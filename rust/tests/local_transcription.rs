use anyhow::{Result, ensure};
use meditor_playback::project::import_srt;
use std::{env, fs, path::Path, process::Command};

fn write_wave(audio: &Path, sample_rate: u32) -> Result<()> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut wave = hound::WavWriter::create(audio, spec)?;
    for _ in 0..2 * sample_rate {
        wave.write_sample(0_i16)?;
    }
    wave.finalize()?;
    Ok(())
}

#[test]
fn local_srt_roundtrip_and_failures() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let root = directory.path();
    let engine = root.join(format!("mock whisper{}", env::consts::EXE_SUFFIX));
    let source = root.join("whisper_fixture.rs");
    fs::write(&source, include_str!("fixtures/mock_whisper.rs"))?;
    let compile = Command::new(env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .args(["--edition", "2024"])
        .arg(source)
        .arg("-o")
        .arg(&engine)
        .output()?;
    ensure!(
        compile.status.success(),
        "{}",
        String::from_utf8_lossy(&compile.stderr)
    );

    let audio = root.join("voice sample.wav");
    write_wave(&audio, 16_000)?;
    let original_audio = fs::read(&audio)?;
    let run = |model: &str, output: &str| {
        let model = root.join(model);
        fs::write(&model, b"mock model")?;
        Command::new(env!("CARGO_BIN_EXE_meditor-transcribe"))
            .current_dir(root)
            .arg(engine.file_name().unwrap())
            .arg(model.file_name().unwrap())
            .arg(audio.file_name().unwrap())
            .arg(output)
            .output()
    };

    let success = run("multilingual model.bin", "字幕.srt")?;
    assert!(
        success.status.success(),
        "{}",
        String::from_utf8_lossy(&success.stderr)
    );
    let result = root.join("字幕.srt");
    let exported = fs::read(&result)?;
    let captions = import_srt(&result)?;
    assert_eq!(captions.len(), 1);
    assert_eq!(captions[0].text, "こんにちは。");
    assert_eq!(
        (captions[0].start_us, captions[0].end_us),
        (150_000, 1_100_000)
    );
    assert_eq!(fs::read(&audio)?, original_audio);

    let existing = run("engine failure.bin", "字幕.srt")?;
    assert!(!existing.status.success());
    assert!(String::from_utf8_lossy(&existing.stderr).contains("既に存在"));
    assert_eq!(fs::read(&result)?, exported);

    for (model, error) in [
        ("engine failure.bin", "文字起こしに失敗"),
        ("invalid output.bin", "SRT を読み込めません"),
        ("no speech.bin", "発話が見つかりません"),
        ("no output.bin", "SRT を生成しません"),
    ] {
        let result = run(model, "failed.srt")?;
        assert!(!result.status.success(), "{model}");
        let stderr = String::from_utf8_lossy(&result.stderr);
        assert!(stderr.contains(error), "{stderr}");
        assert!(!root.join("failed.srt").exists(), "{model}");
    }

    let race = run("output race.bin", "race.srt")?;
    assert!(!race.status.success());
    assert!(String::from_utf8_lossy(&race.stderr).contains("SRT を確定できません"));
    assert_eq!(fs::read(root.join("race.srt"))?, b"another writer");

    write_wave(&audio, 48_000)?;
    let unsupported = run("engine failure.bin", "wrong-rate.srt")?;
    assert!(!unsupported.status.success());
    assert!(String::from_utf8_lossy(&unsupported.stderr).contains("モノラル 16 kHz"));
    assert!(!root.join("wrong-rate.srt").exists());
    Ok(())
}
