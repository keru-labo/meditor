//! Tracks subtitle intervals against the most recently presented video frame.

use meditor_playback::project::Caption;

#[derive(Default)]
pub struct CaptionOverlay {
    max_end: Vec<i64>,
    leaves: usize,
    frame_time: Option<i64>,
    active: Vec<usize>,
    scratch: Vec<usize>,
}

impl CaptionOverlay {
    pub fn rebuild(&mut self, captions: &[Caption]) -> String {
        // The project reader and SRT importer keep rows in stable start-time order.
        self.leaves = captions.len().next_power_of_two();
        self.max_end = vec![i64::MIN; self.leaves * 2];
        for (index, caption) in captions.iter().enumerate() {
            self.max_end[self.leaves + index] = caption.end_us;
        }
        for node in (1..self.leaves).rev() {
            self.max_end[node] = self.max_end[node * 2].max(self.max_end[node * 2 + 1]);
        }
        self.find_active(captions);
        std::mem::swap(&mut self.active, &mut self.scratch);
        caption_text(captions, &self.active)
    }

    pub fn clear_frame(&mut self) {
        self.frame_time = None;
        self.active.clear();
    }

    pub fn present(&mut self, captions: &[Caption], timeline_us: i64) -> Option<String> {
        if self.frame_time == Some(timeline_us) {
            return None;
        }
        self.frame_time = Some(timeline_us);
        self.find_active(captions);
        if self.active == self.scratch {
            return None;
        }
        std::mem::swap(&mut self.active, &mut self.scratch);
        Some(caption_text(captions, &self.active))
    }

    pub fn edit(&self, captions: &[Caption], row: usize) -> Option<String> {
        self.active
            .binary_search(&row)
            .ok()
            .map(|_| caption_text(captions, &self.active))
    }

    fn find_active(&mut self, captions: &[Caption]) {
        self.scratch.clear();
        let Some(time) = self.frame_time else { return };
        let limit = captions.partition_point(|caption| caption.start_us <= time);
        if limit > 0 {
            visit_intervals(
                &self.max_end,
                1,
                0..self.leaves,
                limit,
                time,
                &mut self.scratch,
            );
        }
    }
}

fn visit_intervals(
    max_end: &[i64],
    node: usize,
    rows: std::ops::Range<usize>,
    limit: usize,
    time: i64,
    active: &mut Vec<usize>,
) {
    if rows.start >= limit || max_end[node] <= time {
        return;
    }
    if rows.len() == 1 {
        active.push(rows.start);
        return;
    }

    let middle = rows.start + rows.len() / 2;
    visit_intervals(max_end, node * 2, rows.start..middle, limit, time, active);
    visit_intervals(max_end, node * 2 + 1, middle..rows.end, limit, time, active);
}

fn caption_text(captions: &[Caption], active: &[usize]) -> String {
    let mut text = String::new();
    for &row in active {
        let cue = &captions[row];
        if cue.text.is_empty() {
            continue;
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&cue.text);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cue(start_us: i64, end_us: i64, speaker: &str, text: &str) -> Caption {
        Caption {
            id: format!("{start_us}:{end_us}:{text}"),
            start_us,
            end_us,
            speaker: speaker.to_owned(),
            text: text.to_owned(),
        }
    }

    #[test]
    fn displays_overlapping_cues_and_excludes_ends_in_both_directions() {
        let cues = [
            cue(0, 20, "話者A", "first\nline"),
            cue(10, 30, "", "overlap"),
            cue(20, 25, "話者B", "A: replacement"),
            cue(20, 25, "話者C", ""),
        ];
        let mut overlay = CaptionOverlay::default();
        assert_eq!(overlay.rebuild(&cues), "");
        assert_eq!(overlay.present(&cues, 0).unwrap(), "first\nline");
        assert_eq!(overlay.present(&cues, 9), None);
        assert_eq!(overlay.present(&cues, 10).unwrap(), "first\nline\noverlap");
        assert_eq!(
            overlay.present(&cues, 20).unwrap(),
            "overlap\nA: replacement"
        );
        assert_eq!(overlay.present(&cues, 25).unwrap(), "overlap");
        assert_eq!(overlay.present(&cues, 30).unwrap(), "");
        assert_eq!(overlay.present(&cues, 10).unwrap(), "first\nline\noverlap");
    }

    #[test]
    fn retains_long_intervals_and_refreshes_active_edits_and_replacements() {
        let mut cues = vec![cue(0, 25_000, "", "long")];
        cues.extend((1..10_000).map(|start| cue(start, start + 1, "", "one frame")));
        let mut overlay = CaptionOverlay::default();
        overlay.rebuild(&cues);
        assert_eq!(overlay.present(&cues, 10_001).unwrap(), "long");
        assert_eq!(overlay.edit(&cues, 9_999), None);
        cues[0].text = "edited".into();
        assert_eq!(overlay.edit(&cues, 0).unwrap(), "edited");
        assert_eq!(overlay.rebuild(&[cue(0, 20_000, "", "new")]), "new");
        overlay.clear_frame();
        assert_eq!(overlay.rebuild(&[cue(0, 20_000, "", "new")]), "");
    }
}
