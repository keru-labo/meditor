use std::{env, fs, path::Path, process};

fn main() -> std::io::Result<()> {
    let args: Vec<_> = env::args_os().skip(1).collect();
    assert_eq!(args.len(), 10);
    assert_eq!(args[0], "--model");
    assert_eq!(args[2], "--file");
    assert_eq!(
        args[4..],
        [
            "--language",
            "ja",
            "--no-prints",
            "--output-srt",
            "--output-file",
            "transcript",
        ]
    );
    let model = Path::new(&args[1]);
    let audio = Path::new(&args[3]);
    assert!(model.is_file() && model.is_absolute());
    assert!(audio.is_file() && audio.is_absolute());
    assert_eq!(audio.file_name().unwrap(), "voice sample.wav");

    const TRANSCRIPT: &str = "1\n00:00:00,150 --> 00:00:01,100\nこんにちは。\n\n";
    match model.file_name().and_then(|file| file.to_str()).unwrap() {
        "engine failure.bin" => {
            fs::write("transcript.srt", TRANSCRIPT)?;
            process::exit(4);
        }
        "invalid output.bin" => fs::write("transcript.srt", b"not an SRT")?,
        "no speech.bin" => fs::write("transcript.srt", b"")?,
        "no output.bin" => {}
        "output race.bin" => {
            fs::write("../race.srt", b"another writer")?;
            fs::write("transcript.srt", TRANSCRIPT)?;
        }
        "multilingual model.bin" => fs::write("transcript.srt", TRANSCRIPT)?,
        other => panic!("unexpected fixture: {other}"),
    }
    Ok(())
}
