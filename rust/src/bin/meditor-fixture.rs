use anyhow::{Context, Result, ensure};
use meditor_playback::project::{Caption, Project, Track};
use std::{fs, path::PathBuf, process::Command};

fn encode(args: &[&str], output: &std::path::Path) -> Result<()> {
    let status = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .arg(output)
        .status()
        .context("FFmpeg CLI が PATH に必要です")?;
    ensure!(
        status.success(),
        "合成素材を作成できません ({})",
        output.display()
    );
    Ok(())
}

fn main() -> Result<()> {
    let directory = std::path::absolute(PathBuf::from(
        std::env::args_os()
            .nth(1)
            .context("meditor-fixture OUTPUT_DIRECTORY")?,
    ))?;
    fs::create_dir_all(&directory)?;
    let video = directory.join("original.mp4");
    let proxy = directory.join("proxy.mp4");
    encode(
        &[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=60",
            "-threads",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-g",
            "120",
            "-keyint_min",
            "120",
            "-sc_threshold",
            "0",
        ],
        &video,
    )?;
    encode(
        &[
            "-i",
            video.to_str().context("UTF-8 の出力フォルダーが必要です")?,
            "-an",
            "-vf",
            "scale=480:-2",
            "-threads",
            "2",
            "-c:v",
            "libx264",
            "-preset",
            "veryfast",
            "-g",
            "6",
            "-keyint_min",
            "6",
            "-sc_threshold",
            "0",
            "-bf",
            "0",
        ],
        &proxy,
    )?;
    let mut tracks = Vec::new();
    for (file, name, frequency, offset_us, gain) in [
        ("game", "ゲーム音", 220, 0, 0.30),
        ("voice-a", "話者 A", 440, 0, 0.55),
        ("voice-b", "話者 B", 660, 120_000, 0.55),
    ] {
        let path = directory.join(format!("{file}.wav"));
        let source = format!("sine=frequency={frequency}:sample_rate=48000:duration=60");
        encode(
            &[
                "-f",
                "lavfi",
                "-i",
                &source,
                "-ac",
                "2",
                "-c:a",
                "pcm_s16le",
            ],
            &path,
        )?;
        tracks.push(Track {
            path,
            name: name.into(),
            offset_us,
            gain,
        });
    }
    let mut project = Project::new(video, tracks)?;
    project.proxy = proxy;
    project.captions = (0..10_000)
        .map(|index| Caption {
            id: (index + 1).to_string(),
            start_us: index * 6_000,
            end_us: index * 6_000 + 350_000,
            speaker: if index % 2 == 0 {
                "話者 B"
            } else {
                "話者 A"
            }
            .into(),
            text: format!("実況字幕 {}", index + 1),
        })
        .collect();
    project.save(&directory.join("fixture.json"))
}
