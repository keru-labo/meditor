use super::{DialogAnswer, Editor, EditorWindow, Step, check, input};
use anyhow::{Context, Result, ensure};
use meditor_playback::project::{Caption, Project};
use slint::{ComponentHandle, platform::Key};
use std::{cell::RefCell, path::Path, process::Command};

pub(super) fn execute(
    ui: &EditorWindow,
    editor: &RefCell<Editor>,
    destination: &Path,
    stage: usize,
) -> Result<Step> {
    let (action, operations): (_, &[_]) = match stage {
        0 => {
            let video = destination.with_file_name("subtitle-colors.mp4");
            let status = Command::new("ffmpeg").args([
                "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi", "-i",
                "color=c=blue:s=640x360:r=30:d=1,drawbox=x=0:y=0:w=320:h=360:color=white:t=fill",
                "-c:v", "libx264", "-threads", "1", "-pix_fmt", "yuv420p",
            ]).arg(&video).status().context("テスト映像を作成できません")?;
            ensure!(status.success(), "テスト映像を作成できません");
            let mut project = Project::new(video, Vec::new())?;
            project.captions = [("話者A", "行こう"), ("話者B", "待って")]
                .into_iter()
                .enumerate()
                .map(|(index, (speaker, text))| Caption {
                    id: (index + 1).to_string(),
                    start_us: 0,
                    end_us: 1_000_000,
                    speaker: speaker.into(),
                    text: text.into(),
                })
                .collect();
            let source = destination.with_file_name("subtitle-colors.json");
            project.save(&source)?;
            super::open_project(ui, editor, &source, destination)?;
            (
                "同じ時刻の2つの字幕をプレビューで確認する",
                &["白と青の合成映像に話者Aと話者Bの字幕を重ねて開く"],
            )
        }
        1 => {
            input::click(ui, "subtitle-style")?;
            input::replace(ui, "style-font", "Noto Sans CJK JP")?;
            input::replace(ui, "style-size", "30")?;
            input::replace(ui, "style-fill", "#FFFF00")?;
            input::replace(ui, "style-outline", "#00FF00")?;
            input::replace(ui, "style-thickness", "5")?;
            input::click(ui, "style-apply")?;
            (
                "文字色と縁取りを適用し、プレビューの画素を照合する",
                &[
                    "フォントをNoto Sans CJK JP、文字サイズを30にする",
                    "黄色の文字と緑の太い縁取りを適用する",
                ],
            )
        }
        2 => {
            input::click(ui, "export")?;
            input::click(ui, "export-canvas")?;
            input::key(ui, Key::UpArrow)?;
            input::key(ui, Key::UpArrow)?;
            input::key(ui, Key::Return)?;
            let path = destination.with_file_name("subtitle-pixels-export.mp4");
            super::super::click_with_files(
                ui,
                editor,
                "export-save",
                [DialogAnswer::File(Some(path))],
            )?;
            (
                "字幕を焼き込んだMP4を保存する",
                &["元の映像に合わせるを選び、書き出す"],
            )
        }
        _ => anyhow::bail!("Unknown pixel inspection stage {stage}"),
    };
    Ok(Step {
        action,
        operations,
        rationale: "字幕の周囲にある映像色と、実際に描かれた文字・縁取りの画素を確認する。",
        checks: Vec::new(),
        screenshot: String::new(),
        spec: Vec::new(),
    })
}

fn rgb(pixel: [u8; 3]) -> Option<&'static str> {
    let [r, g, b] = pixel;
    if r >= 190 && g >= 185 && b <= 120 {
        return Some("yellow");
    }
    if r <= 150 && g >= 170 && b <= 135 {
        return Some("green");
    }
    None
}

fn white([r, g, b]: [u8; 3]) -> bool {
    r >= 190 && g >= 190 && b >= 190
}
fn blue([r, g, b]: [u8; 3]) -> bool {
    r < 75 && g < 75 && b > 140
}

fn inspect_region(
    xs: std::ops::Range<usize>,
    ys: std::ops::Range<usize>,
    pixel: impl Fn(usize, usize) -> [u8; 3],
    styled: bool,
) -> ([usize; 2], bool) {
    let mut glyph_bounds = None::<(usize, usize, usize, usize)>;
    let mut colors = [0; 2];
    for y in ys {
        for x in xs.clone() {
            let color = pixel(x, y);
            let tone = rgb(color);
            if tone == Some("yellow") {
                colors[0] += 1;
            }
            if tone == Some("green") {
                colors[1] += 1;
            }
            let glyph = if styled {
                tone.is_some()
            } else {
                color.iter().all(|&channel| channel < 60)
            };
            if glyph {
                glyph_bounds = Some(match glyph_bounds {
                    Some((left, top, right, bottom)) => {
                        (left.min(x), top.min(y), right.max(x), bottom.max(y))
                    }
                    None => (x, y, x, y),
                });
            }
        }
    }
    let Some((left, top, right, bottom)) = glyph_bounds else {
        return (colors, false);
    };
    let area = (right - left + 1) * (bottom - top + 1);
    let mut remaining = [0; 2];
    let mut dark_pixels = 0;
    let mut longest_dark_run = 0;
    for y in top..=bottom {
        let mut dark_run = 0;
        for x in left..=right {
            let color = pixel(x, y);
            if white(color) {
                remaining[0] += 1;
            }
            if blue(color) {
                remaining[1] += 1;
            }
            if color.iter().all(|&channel| channel < 60) {
                dark_pixels += 1;
                dark_run += 1;
                longest_dark_run = longest_dark_run.max(dark_run);
            } else {
                dark_run = 0;
            }
        }
    }
    // Styled glyphs and the two source colors contain no dark backing. Only
    // the default outline permits thin dark strokes alongside the lettering.
    let no_backdrop = if styled {
        dark_pixels <= area / 100
    } else {
        longest_dark_run * 4 < (right - left + 1) * 3
    };
    (
        colors,
        no_backdrop && remaining.iter().all(|&pixels| pixels > area / 40),
    )
}

fn yes(condition: bool) -> &'static str {
    if condition { "はい" } else { "いいえ" }
}

pub(super) fn observe(
    ui: &EditorWindow,
    destination: &Path,
    stage: usize,
    step: &mut Step,
) -> Result<()> {
    if stage == 2 {
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(destination.with_file_name("subtitle-pixels-export.mp4"))
            .args([
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "-pix_fmt",
                "rgb24",
                "-threads",
                "1",
                "pipe:1",
            ])
            .output()
            .context("MP4の画素を取得できません")?;
        let good = decoded.status.success() && decoded.stdout.len() == 640 * 360 * 3;
        let pixel = |x: usize, y: usize| {
            let i = (y * 640 + x) * 3;
            [
                decoded.stdout[i],
                decoded.stdout[i + 1],
                decoded.stdout[i + 2],
            ]
        };
        let (colors, background) = if good {
            inspect_region(165..475, 235..360, pixel, true)
        } else {
            ([0; 2], false)
        };
        step.checks.extend([
            check(
                "MP4を生成した",
                "はい",
                yes(good && ui.get_status().starts_with("書き出しました:")),
            ),
            check(
                "MP4に黄色と緑の字幕画素がある",
                "はい",
                yes(colors[0] > 10 && colors[1] > 30),
            ),
            check(
                "MP4の字幕の周囲にも白と青の映像が見える",
                "はい",
                yes(background),
            ),
        ]);
        return Ok(());
    }
    let shot = ui.window().take_snapshot()?;
    let (width, height) = (shot.width() as usize, shot.height() as usize);
    ensure!(
        width >= 600 && height >= 500,
        "字幕を検査できる画面サイズが必要です"
    );
    let pixel = |x: usize, y: usize| {
        let p = shot.as_slice()[(y * height / 740) * width + x * width / 1180];
        [p.r, p.g, p.b]
    };
    let (colors, background) = inspect_region(220..495, 370..495, pixel, stage == 1);
    step.checks.extend([
        check(
            "プレビューの字幕には本文だけがある",
            "行こう\n待って",
            ui.get_preview_caption(),
        ),
        check(
            "字幕一覧の話者は区別される",
            "話者A,話者B",
            {
                use slint::Model;
                let rows = ui.get_captions();
                format!(
                    "{},{}",
                    rows.row_data(0).context("字幕1行目なし")?.speaker,
                    rows.row_data(1).context("字幕2行目なし")?.speaker
                )
            },
        ),
        check("字幕の周囲に白と青の映像が見える", "はい", yes(background)),
    ]);
    if stage == 1 {
        let appearance = ui.get_caption_appearance();
        step.checks.push(check(
            "適用したフォント名と文字サイズ",
            "Noto Sans CJK JP,30",
            format!("{},{}", appearance.font_family, appearance.font_size),
        ));
        step.checks.push(check(
            "プレビューに黄色と緑の字幕画素がある",
            "はい",
            yes(colors[0] > 15 && colors[1] > 30),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::inspect_region;

    #[test]
    fn separate_black_caption_boxes_do_not_pass_as_transparent() {
        let canvas = |x, y, boxed| {
            let line = (10..20).contains(&y) || (35..45).contains(&y);
            let in_text = (16..64).contains(&x) && line;
            if in_text && x % 5 == 0 {
                return [255, 255, 0];
            }
            if in_text && x % 5 == 1 {
                return [0, 255, 0];
            }
            if in_text && boxed {
                return [0, 0, 0];
            }
            if x < 40 { [255, 255, 255] } else { [0, 0, 255] }
        };
        assert!(inspect_region(0..80, 0..60, |x, y| canvas(x, y, false), true).1);
        assert!(!inspect_region(0..80, 0..60, |x, y| canvas(x, y, true), true).1);
    }
}
