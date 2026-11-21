use anyhow::{Result, bail};
use meditor_playback::transcription::transcribe;
use std::path::{Path, PathBuf};

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
