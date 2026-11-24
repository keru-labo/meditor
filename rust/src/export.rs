use crate::ExportSelection;
use crate::audio::write_export_mix;
use anyhow::{Context, Result, ensure};
use meditor_playback::{
    ai::{ExportCanvas, ExportPreferences, ExportQuality},
    project::{Caption, ClipRange, Project, SECOND, Track},
    subtitle_style::SubtitleStyle,
};
use std::{
    fs::{self, File},
    path::PathBuf,
    process::{Command, Stdio},
};

const CAPTION_HEADER: &str = "[Script Info]\n\
    ScriptType: v4.00+\nPlayResX: 640\nPlayResY: 360\nWrapStyle: 0\n\
    [V4+ Styles]\n\
    Format: Name,Fontname,Fontsize,PrimaryColour,SecondaryColour,OutlineColour,BackColour,Bold,Italic,Underline,StrikeOut,ScaleX,ScaleY,Spacing,Angle,BorderStyle,Outline,Shadow,Alignment,MarginL,MarginR,MarginV,Encoding\n";
const CAPTION_EVENTS: &str =
    "[Events]\nFormat: Layer,Start,End,Style,Name,MarginL,MarginR,MarginV,Effect,Text\n";

#[derive(Clone, Copy, Default)]
pub(crate) struct ExportSettings {
    quality: Quality,
    canvas: Canvas,
}

#[derive(Clone, Copy, Default)]
enum Quality {
    #[default]
    Standard,
    High,
    Compact,
}

#[derive(Clone, Copy, Default)]
enum Canvas {
    #[default]
    Source,
    Landscape,
    Portrait,
}

impl From<ExportPreferences> for ExportSelection {
    fn from(preferences: ExportPreferences) -> Self {
        Self {
            quality: match preferences.quality {
                None | Some(ExportQuality::Standard) => 0,
                Some(ExportQuality::High) => 1,
                Some(ExportQuality::Compact) => 2,
            },
            canvas: match preferences.canvas {
                None | Some(ExportCanvas::Source) => 0,
                Some(ExportCanvas::Landscape) => 1,
                Some(ExportCanvas::Portrait) => 2,
            },
        }
    }
}

impl ExportSettings {
    pub(crate) fn from_selection(quality: i32, canvas: i32) -> Result<Self> {
        let quality = match quality {
            0 => Quality::Standard,
            1 => Quality::High,
            2 => Quality::Compact,
            _ => anyhow::bail!("書き出し画質を選択してください"),
        };
        let canvas = match canvas {
            0 => Canvas::Source,
            1 => Canvas::Landscape,
            2 => Canvas::Portrait,
            _ => anyhow::bail!("書き出しの縦横比を選択してください"),
        };
        Ok(Self { quality, canvas })
    }

    fn crf(self) -> &'static str {
        match self.quality {
            Quality::Standard => "20",
            Quality::High => "18",
            Quality::Compact => "26",
        }
    }

    fn video_filter(self) -> String {
        let (width, height) = match self.canvas {
            Canvas::Source => return "pad=ceil(iw/2)*2:ceil(ih/2)*2".to_owned(),
            Canvas::Landscape => (1280, 720),
            Canvas::Portrait => (720, 1280),
        };
        // Fit the source's display aspect ratio, including non-square source pixels.
        // Round down to even dimensions for YUV 4:2:0, then pad to fill the canvas.
        format!(
            "scale=w='max(2,trunc(min({width},{height}*dar)/2)*2)':\
             h='max(2,trunc(min({height},{width}/dar)/2)*2)',\
             setsar=1,pad={width}:{height}:(ow-iw)/2:(oh-ih)/2"
        )
    }
}

pub(crate) struct ExportSnapshot {
    video: PathBuf,
    stream: usize,
    seek_us: i64,
    range: ClipRange,
    audio: Vec<Track>,
    captions: Vec<Caption>,
    subtitle_style: SubtitleStyle,
    settings: ExportSettings,
}

impl ExportSnapshot {
    pub(crate) fn capture(
        project: &Project,
        duration: i64,
        stream: usize,
        origin_us: i64,
        settings: ExportSettings,
    ) -> Self {
        let range = project.playback_range(duration);
        let caption_start = project.caption_time_offset(range);
        let caption_end = caption_start + range.duration();
        let captions = project
            .captions
            .iter()
            .filter(|caption| caption.start_us < caption_end && caption.end_us > caption_start)
            .cloned()
            .map(|mut caption| {
                caption.start_us = caption.start_us.max(caption_start) - caption_start;
                caption.end_us = caption.end_us.min(caption_end) - caption_start;
                caption
            })
            .collect();
        Self {
            video: project.video.clone(),
            stream,
            seek_us: origin_us.saturating_add(range.source_start_us),
            range,
            audio: project.audio.clone(),
            captions,
            subtitle_style: project.subtitle_style.clone(),
            settings,
        }
    }

    pub(crate) fn write(self, mut destination: PathBuf) -> Result<PathBuf> {
        if destination.extension().is_none() {
            destination.set_extension("mp4");
        }
        ensure!(
            destination
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("mp4")),
            "書き出し先には .mp4 を指定してください"
        );
        let destination = std::path::absolute(destination)?;
        ensure!(!destination.exists(), "既存のファイルは上書きしません");
        let parent = destination
            .parent()
            .context("書き出し先のフォルダーがありません")?;
        let workspace =
            tempfile::tempdir_in(parent).context("書き出し先に一時フォルダーを作成できません")?;
        let output = tempfile::NamedTempFile::new_in(workspace.path())?.into_temp_path();
        let video = std::path::absolute(&self.video)?;
        let log = workspace.path().join("ffmpeg.log");
        let mut command = Command::new(ffmpeg_on_path()?);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // CREATE_NO_WINDOW keeps the console hidden while the GUI is active.
            command.creation_flags(0x0800_0000);
        }
        command
            .current_dir(workspace.path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-xerror",
                "-nostdin",
                "-y",
            ])
            .args([
                "-abort_on",
                "empty_output_stream",
                "-protocol_whitelist",
                "file",
            ])
            .args(["-seek_timestamp", "1", "-ss", &seconds(self.seek_us), "-i"])
            .arg(video)
            .stdout(Stdio::null())
            .stderr(File::create(&log)?);
        if self.audio.is_empty() {
            command.stdin(Stdio::null()).arg("-an");
        } else {
            command
                .stdin(Stdio::piped())
                .args(["-f", "s16le", "-ar", "48000", "-ac", "2", "-i", "pipe:0"])
                .args(["-map", "1:a:0", "-c:a", "aac", "-b:a", "192k"]);
        }
        let mut video_filter = self.settings.video_filter();
        if !self.captions.is_empty() {
            fs::write(
                workspace.path().join("captions.ass"),
                caption_script(&self.captions, self.range.duration(), &self.subtitle_style)?,
            )?;
            // Subtitle path stays constant, including when the project path contains filter syntax.
            video_filter.push_str(",ass=filename=captions.ass");
        }
        command
            .args([
                "-map",
                &format!("0:{}", self.stream),
                "-vf",
                &video_filter,
                "-t",
            ])
            .arg(seconds(self.range.duration()))
            .args([
                "-c:v",
                "libx264",
                "-preset",
                "veryfast",
                "-crf",
                self.settings.crf(),
                "-pix_fmt",
                "yuv420p",
                "-movflags",
                "+faststart",
                "-f",
                "mp4",
            ])
            .arg(output.file_name().unwrap());

        let mut child = command
            .spawn()
            .context("書き出しには PATH 上の FFmpeg が必要です")?;
        let mix = if let Some(stdin) = child.stdin.take() {
            write_export_mix(self.audio, self.range, stdin)
        } else {
            Ok(())
        };
        // Always reap FFmpeg and remove the staged output on failure.
        let status = child.wait().context("FFmpeg の書き出し結果を待てません")?;
        if let Err(error) = mix {
            let closed = error
                .downcast_ref::<std::io::Error>()
                .is_some_and(|e| e.kind() == std::io::ErrorKind::BrokenPipe);
            if !closed || status.success() {
                return Err(error.context("書き出し用音声を読み込めません"));
            }
        }
        if !status.success() {
            let reason = fs::read_to_string(&log).unwrap_or_default();
            let first_error = reason.lines().find(|line| !line.is_empty()).unwrap_or("");
            anyhow::bail!("FFmpeg で書き出せませんでした ({status}) {first_error}");
        }
        output
            .persist_noclobber(&destination)
            .context("書き出し先に新しいファイルを保存できません")?;
        Ok(destination)
    }
}

fn ffmpeg_on_path() -> Result<PathBuf> {
    let program = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
    std::env::var_os("PATH")
        .and_then(|path| {
            std::env::split_paths(&path)
                .filter(|dir| dir.is_absolute())
                .map(|dir| dir.join(&program))
                .find(|entry| entry.is_file())
        })
        .context("書き出しには PATH 上の FFmpeg が必要です")
}

fn seconds(micros: i64) -> String {
    let magnitude = micros.unsigned_abs();
    let sign = if micros < 0 { "-" } else { "" };
    format!(
        "{sign}{}.{:06}",
        magnitude / SECOND as u64,
        magnitude % SECOND as u64
    )
}

fn ass_time(micros: i64) -> String {
    let units = (micros + 9_999) / 10_000;
    let seconds = units / 100;
    format!(
        "{}:{:02}:{:02}.{:02}",
        seconds / 3600,
        seconds / 60 % 60,
        seconds % 60,
        units % 100
    )
}

fn ass_color([red, green, blue]: [u8; 3]) -> String {
    format!("&H00{blue:02X}{green:02X}{red:02X}")
}

fn caption_script(captions: &[Caption], duration: i64, style: &SubtitleStyle) -> Result<String> {
    use std::fmt::Write as _;
    style.validate()?;
    let mut script = CAPTION_HEADER.to_owned();
    let text_color = ass_color(style.text_color);
    let outline_color = ass_color(style.outline_color);
    // BorderStyle 1 outlines glyphs without a rectangular backing. Shadow is disabled.
    let _ = writeln!(
        script,
        "Style: Default,{},{},{text_color},{text_color},{outline_color},&HFF000000,0,0,0,0,100,100,0,0,1,{},0,2,16,16,20,1",
        style.font_family, style.font_size, style.outline_width,
    );
    script.push_str(CAPTION_EVENTS);
    for caption in captions {
        let end = caption.end_us.min(duration);
        if caption.text.is_empty() || caption.start_us >= end {
            continue;
        }
        let text = caption.text.replace("\r\n", "\n").replace('\r', "\n");
        let mut encoded = String::with_capacity(text.len());
        for ch in text.chars() {
            match ch {
                '\n' => encoded.push_str("\\N"),
                '\\' => encoded.push_str("\\\u{2060}"),
                '{' | '}' => {
                    encoded.push('\\');
                    encoded.push(ch);
                }
                '\t' => encoded.push(' '),
                ch if ch.is_control() => {}
                ch => encoded.push(ch),
            }
        }
        let _ = writeln!(
            script,
            "Dialogue: 0,{},{},Default,,0,0,0,,{encoded}",
            ass_time(caption.start_us),
            ass_time(end),
        );
    }
    Ok(script)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, path::Path};

    fn ffmpeg(args: &[&str], path: &Path) -> Result<Vec<u8>> {
        let output = Command::new(ffmpeg_on_path()?)
            .args(["-hide_banner", "-loglevel", "error", "-nostdin"])
            .args(args)
            .arg(path)
            .output()?;
        ensure!(
            output.status.success(),
            "FFmpeg fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        Ok(output.stdout)
    }

    #[test]
    fn caption_script_keeps_plain_text_and_clips_to_timeline_end() -> Result<()> {
        let cue = Caption {
            id: "one".into(),
            start_us: 100_000,
            end_us: 3_000_000,
            speaker: "話者A".into(),
            text: "A: first\r\n{\\pos(1,1)}".into(),
        };
        let empty = Caption {
            id: "empty".into(),
            speaker: "話者B".into(),
            text: String::new(),
            ..cue.clone()
        };
        let script = caption_script(&[cue, empty], 2_000_000, &SubtitleStyle::default())?;
        assert!(script.contains("0:00:00.10,0:00:02.00"));
        assert!(script.contains(",,A: first\\N\\{\\\u{2060}pos(1,1)\\}"));
        assert!(!script.contains("話者"));
        assert_eq!(script.matches("Dialogue:").count(), 1);
        Ok(())
    }

    #[test]
    fn caption_script_uses_opaque_bgr_glyph_colors_without_a_box() -> Result<()> {
        let mut style = SubtitleStyle {
            font_family: "Yu Gothic UI".into(),
            font_size: 40,
            text_color: [0x12, 0x34, 0x56],
            outline_color: [0xAB, 0xCD, 0xEF],
            outline_width: 0,
        };
        let script = caption_script(&[], 1_000_000, &style)?;
        assert!(script.contains(
            "Style: Default,Yu Gothic UI,40,&H00563412,&H00563412,&H00EFCDAB,&HFF000000,0,0,0,0,100,100,0,0,1,0,0,2,16,16,20,1\n"
        ));
        style.font_family = "Arial,24".into();
        assert!(caption_script(&[], 1_000_000, &style).is_err());
        Ok(())
    }

    #[test]
    fn mp4_burns_the_captured_text_and_outline_colors() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("subtitle-style.mp4");
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=0x238B45:size=320x180:rate=5:duration=0.4",
                "-c:v",
                "libx264",
            ],
            &source,
        )?;
        let mut project = Project::new(source, vec![])?;
        project.captions.push(Caption {
            id: "title".into(),
            start_us: 0,
            end_us: 400_000,
            speaker: String::new(),
            text: "STYLE".into(),
        });
        project.subtitle_style = SubtitleStyle {
            font_size: 72,
            text_color: [255, 240, 0],
            outline_color: [255, 0, 255],
            outline_width: 12,
            ..SubtitleStyle::default()
        };
        let snapshot = ExportSnapshot::capture(&project, 400_000, 0, 0, ExportSettings::default());
        project.subtitle_style = SubtitleStyle::default();
        let output = snapshot.write(directory.path().join("styled.mp4"))?;

        let pixels = ffmpeg(
            &[
                "-i",
                output.to_str().unwrap(),
                "-frames:v",
                "1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "-threads",
                "1",
            ],
            Path::new("pipe:1"),
        )?;
        assert_eq!(pixels.len(), 320 * 180 * 3);
        let (pixels, _) = pixels.as_chunks::<3>();
        assert!(
            pixels
                .iter()
                .any(|&[red, green, blue]| red > 170 && green > 150 && blue < 90),
            "expected yellow glyphs in the decoded frame"
        );
        assert!(
            pixels
                .iter()
                .any(|&[red, green, blue]| red > 170 && green < 90 && blue > 160),
            "expected a magenta outline in the decoded frame"
        );
        Ok(())
    }

    #[test]
    fn source_captions_are_trimmed_and_shifted_to_the_clip_timeline() -> Result<()> {
        let mut project = Project::new("fixture.mp4".into(), vec![])?;
        project.set_clip(
            ClipRange {
                source_start_us: 1_000_000,
                source_end_us: 3_000_000,
            },
            4_000_000,
        )?;
        project.captions_use_source_time = true;
        for (text, start_us, end_us) in [
            ("before", 500_000, 1_000_000),
            ("left", 800_000, 1_200_000),
            ("inside", 1_200_000, 2_000_000),
            ("right", 2_800_000, 3_200_000),
            ("after", 3_000_000, 3_400_000),
        ] {
            project.captions.push(Caption {
                id: text.into(),
                start_us,
                end_us,
                speaker: String::new(),
                text: text.into(),
            });
        }
        let snapshot =
            ExportSnapshot::capture(&project, 4_000_000, 0, 0, ExportSettings::default());
        assert_eq!(
            snapshot
                .captions
                .iter()
                .map(|cue| (cue.text.as_str(), cue.start_us, cue.end_us))
                .collect::<Vec<_>>(),
            [
                ("left", 0, 200_000),
                ("inside", 200_000, 1_000_000),
                ("right", 1_800_000, 2_000_000),
            ],
        );
        Ok(())
    }

    #[test]
    fn mp4_contains_burned_in_captions_and_the_offset_wav_mix() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("元動画 (1).mp4");
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=black:size=192x108:rate=10:duration=3",
                "-vf",
                "drawbox=x=0:y=0:w=48:h=32:color=white:t=fill:enable='lt(t,0.5)+gte(t,2.5)'",
                "-c:v",
                "libx264",
            ],
            &source,
        )?;
        let wav = directory.path().join("話者.wav");
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&wav, spec)?;
        for frame in 0..96_000 {
            let sample = ((frame as f32 * std::f32::consts::TAU / 100.0).sin() * 7_000.0) as i16;
            writer.write_sample(sample)?;
            writer.write_sample(sample)?;
        }
        writer.finalize()?;
        let mut project = Project::new(
            source,
            vec![Track {
                path: wav,
                name: "話者A".into(),
                offset_us: 900_000,
                gain: 0.5,
            }],
        )?;
        project.set_clip(
            ClipRange {
                source_start_us: 500_000,
                source_end_us: 2_500_000,
            },
            3_000_000,
        )?;
        project.captions.push(Caption {
            id: "one".into(),
            start_us: 200_000,
            end_us: 900_000,
            speaker: "話者A".into(),
            text: "{\\an9}実況\nSecond line".into(),
        });
        let snapshot =
            ExportSnapshot::capture(&project, 3_000_000, 0, 0, ExportSettings::default());
        project.captions.clear();
        project.audio.clear();
        let output = snapshot.write(directory.path().join("書き出し '日本語'.mp4"))?;
        let frame = |time| -> Result<Vec<u8>> {
            let pixels = ffmpeg(
                &[
                    "-ss",
                    time,
                    "-sn",
                    "-i",
                    output.to_str().unwrap(),
                    "-frames:v",
                    "1",
                    "-f",
                    "rawvideo",
                    "-pix_fmt",
                    "gray",
                    "-threads",
                    "1",
                ],
                Path::new("pipe:1"),
            )?;
            assert_eq!(pixels.len(), 192 * 108);
            Ok(pixels)
        };
        let burned = frame("0.4")?;
        let (top, bottom) = burned.split_at(192 * 54);
        // Caption text must appear without an enabled subtitle track or injected top alignment.
        assert!(top.iter().all(|&pixel| pixel < 10));
        assert!(bottom.iter().any(|&pixel| pixel > 80));
        assert!(frame("0.1")?.iter().all(|&pixel| pixel < 10));
        assert!(frame("0.9")?.iter().all(|&pixel| pixel < 10));
        assert!(frame("1.9")?.iter().all(|&pixel| pixel < 10));
        let audio = ffmpeg(
            &[
                "-i",
                output.to_str().unwrap(),
                "-map",
                "0:a:0",
                "-f",
                "s16le",
            ],
            Path::new("pipe:1"),
        )?;
        assert!(audio.len() >= 2 * 48_000 * 2 * 2);
        let peak = |start: usize, end: usize| {
            audio[start * 4..end * 4]
                .as_chunks::<2>()
                .0
                .iter()
                .map(|&pair| i16::from_le_bytes(pair).unsigned_abs())
                .max()
                .unwrap_or_default()
        };
        assert!(peak(4_800, 9_600) < 100, "sound before the offset");
        assert!(peak(38_400, 43_200) > 1_500, "expected shifted sound");

        let existing = directory.path().join("keep.mp4");
        let mut file = File::create(&existing)?;
        file.write_all(b"keep this file")?;
        drop(file);
        let error = ExportSnapshot::capture(&project, 3_000_000, 0, 0, ExportSettings::default())
            .write(existing.clone())
            .unwrap_err();
        assert!(error.to_string().contains("上書き"));
        assert_eq!(fs::read(existing)?, b"keep this file");
        Ok(())
    }

    #[test]
    fn export_settings_validate_selections_and_preserve_the_default() {
        let standard = ExportSettings::from_selection(0, 0).unwrap();
        assert_eq!(standard.crf(), "20");
        assert_eq!(standard.video_filter(), "pad=ceil(iw/2)*2:ceil(ih/2)*2");
        assert_eq!(ExportSettings::from_selection(2, 1).unwrap().crf(), "26");
        assert!(ExportSettings::from_selection(3, 0).is_err());
        assert!(ExportSettings::from_selection(0, -1).is_err());
    }

    #[test]
    fn ai_proposal_prefills_the_same_choices_used_for_export() -> Result<()> {
        let proposed = ExportSelection::from(ExportPreferences {
            quality: Some(ExportQuality::High),
            canvas: Some(ExportCanvas::Portrait),
        });
        let settings = ExportSettings::from_selection(proposed.quality, proposed.canvas)?;
        assert_eq!(settings.crf(), "18");
        assert!(settings.video_filter().contains("pad=720:1280:"));

        let proposed = ExportSelection::from(ExportPreferences {
            quality: None,
            canvas: None,
        });
        let settings = ExportSettings::from_selection(proposed.quality, proposed.canvas)?;
        assert_eq!(settings.crf(), "20");
        assert_eq!(
            settings.video_filter(),
            ExportSettings::default().video_filter()
        );
        Ok(())
    }

    #[test]
    fn portrait_export_keeps_display_aspect_and_applies_chosen_quality() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let source = directory.path().join("anamorphic.mp4");
        ffmpeg(
            &[
                "-f",
                "lavfi",
                "-i",
                "color=white:size=96x64:rate=5:duration=0.4",
                "-vf",
                "setsar=3/2",
                "-c:v",
                "libx264",
            ],
            &source,
        )?;
        let project = Project::new(source, Vec::new())?;
        let settings = ExportSettings::from_selection(1, 2)?;
        let output = ExportSnapshot::capture(&project, 400_000, 0, 0, settings)
            .write(directory.path().join("portrait.mp4"))?;

        ffmpeg_next::init()?;
        let input = ffmpeg_next::format::input(&output)?;
        let stream = input
            .streams()
            .best(ffmpeg_next::media::Type::Video)
            .unwrap();
        let decoder = ffmpeg_next::codec::context::Context::from_parameters(stream.parameters())?
            .decoder()
            .video()?;
        assert_eq!((decoder.width(), decoder.height()), (720, 1280));

        let output_path = output.to_str().unwrap();
        let pixels = ffmpeg(
            &[
                "-i",
                output_path,
                "-frames:v",
                "1",
                "-pix_fmt",
                "gray",
                "-f",
                "rawvideo",
                "-threads",
                "1",
            ],
            Path::new("pipe:1"),
        )?;
        assert_eq!(pixels.len(), 720 * 1280);
        assert!(
            pixels[430 * 720 + 360] < 10,
            "anamorphic image should fit inside the black bars"
        );
        assert!(
            pixels[500 * 720 + 360] > 200,
            "fitted image must be visible"
        );

        let encoded = ffmpeg(
            &[
                "-i",
                output_path,
                "-map",
                "0:v:0",
                "-c:v",
                "copy",
                "-f",
                "h264",
            ],
            Path::new("pipe:1"),
        )?;
        assert!(String::from_utf8_lossy(&encoded).contains("crf=18.0"));
        Ok(())
    }
}
