use anyhow::{Context, Result, bail, ensure};
use meditor_playback::project::{MAX_DURATION, SECOND, import_srt};
use std::{
    ffi::OsStr,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const USAGE: &str = "meditor-transcribe WHISPER_CLI MODEL AUDIO.wav OUTPUT.srt";

fn main() -> Result<()> {
    let paths: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    match paths.as_slice() {
        [argument] if argument == Path::new("--help") || argument == Path::new("-h") => {
            println!("{USAGE}");
            Ok(())
        }
        [engine, model, audio, output] => {
            transcribe(engine, model, audio, output)?;
            println!("字幕を書き出しました ({})", output.display());
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn local_file(path: &Path, label: &str) -> Result<PathBuf> {
    let path = std::path::absolute(path)?;
    ensure!(path.is_file(), "{label}を開けません ({})", path.display());
    Ok(path)
}

fn has_extension(path: &Path, extension: &str) -> bool {
    path.extension()
        .and_then(OsStr::to_str)
        .is_some_and(|value| value.eq_ignore_ascii_case(extension))
}

fn check_wave(path: &Path) -> Result<()> {
    let source = hound::WavReader::open(path).context("音声は PCM WAV 形式で指定してください")?;
    let spec = source.spec();
    ensure!(
        spec.channels == 1
            && spec.sample_rate == 16_000
            && spec.bits_per_sample == 16
            && spec.sample_format == hound::SampleFormat::Int,
        "音声はモノラル 16 kHz PCM s16le WAV で指定してください"
    );
    let duration = i64::from(source.duration()) * SECOND / i64::from(spec.sample_rate);
    ensure!(
        (1..=MAX_DURATION).contains(&duration),
        "音声の長さは 0 秒より長く、24 時間以内にしてください"
    );
    Ok(())
}

fn transcribe(engine: &Path, model: &Path, audio: &Path, output: &Path) -> Result<()> {
    let engine = local_file(engine, "whisper-cli")?;
    if cfg!(windows) {
        ensure!(
            has_extension(&engine, "exe"),
            "whisper-cli.exe を指定してください"
        );
    }
    let model = local_file(model, "モデル")?;
    let audio = local_file(audio, "音声")?;
    let output = std::path::absolute(output)?;
    ensure!(
        has_extension(&output, "srt"),
        "出力名に .srt を指定してください"
    );
    match fs::symlink_metadata(&output) {
        Ok(_) => bail!("出力先は既に存在します ({})", output.display()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("出力先を確認できません"),
    }
    check_wave(&audio)?;

    let parent = output
        .parent()
        .context("出力先のディレクトリがありません")?;
    let staged =
        tempfile::tempdir_in(parent).context("書き出し用の一時ディレクトリを作成できません")?;
    let status = Command::new(&engine)
        .current_dir(staged.path())
        .arg("--model")
        .arg(&model)
        .arg("--file")
        .arg(&audio)
        .args([
            "--language",
            "ja",
            "--no-prints",
            "--output-srt",
            "--output-file",
            "transcript",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .context("whisper-cli を起動できません")?;
    ensure!(
        status.success(),
        "whisper-cli の文字起こしに失敗しました ({status})"
    );

    let generated = staged.path().join("transcript.srt");
    ensure!(
        fs::symlink_metadata(&generated).is_ok_and(|entry| entry.is_file()),
        "whisper-cli が SRT を生成しませんでした"
    );
    let captions = import_srt(&generated).context("whisper-cli の SRT を読み込めません")?;
    ensure!(
        !captions.is_empty(),
        "字幕にできる発話が見つかりませんでした"
    );

    let mut temporary = tempfile::NamedTempFile::new_in(staged.path())?;
    std::io::copy(&mut fs::File::open(&generated)?, &mut temporary)?;
    temporary.as_file().sync_all()?;
    temporary
        .persist_noclobber(&output)
        .map_err(|error| error.error)
        .with_context(|| format!("SRT を確定できません ({})", output.display()))?;
    Ok(())
}
