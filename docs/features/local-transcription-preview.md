# ローカル文字起こしの試用

既存のエディターに読み込める UTF-8 SRT を、選択した音声ファイルから PC 内で生成する。自分で用意した `whisper-cli` と日本語に対応する多言語モデルを使用する。CLI の接続仕様は whisper.cpp v1.8.3 のフラグに合わせている。 [whisper.cpp v1.8.3](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.8.3) · [製品の文字起こし仕様](transcription.md)

## Windows で試す

1. [ビルド手順](../../README.md#ビルド)の環境を用意して、試用コマンドをビルドする。
2. モノラル 16 kHz PCM s16le の WAV を用意する。別形式の音声から変換する場合は、下の FFmpeg コマンドを使える。
3. `whisper-cli.exe` と多言語モデル、WAV、まだ存在しない出力先を指定する。
4. エディターで該当するプロジェクトを開き、**SRT 読込**から生成された字幕を読み込む。音声の先頭とタイムラインの先頭が一致するプロジェクトで確認する。字幕を既に編集したプロジェクトでは、読み込み前に現在の編集を保存する。

```powershell
cargo build --manifest-path rust/Cargo.toml --locked --release --bin meditor-transcribe
ffmpeg -n -i ".\voice.wav" -map 0:a:0 -ac 1 -ar 16000 -c:a pcm_s16le ".\speech.wav"
.\rust\target\release\meditor-transcribe.exe "C:\Tools\whisper\whisper-cli.exe" "C:\Models\ggml-base.bin" ".\speech.wav" ".\speech.srt"
```

既に存在する出力ファイルは上書きしない。実行失敗、SRT の不正な形式、発話が得られなかった場合はエラーを表示する。Windows の v1.8.3 では実行時のコードページで表現できる WAV とモデルのパスを使う。

## 試用範囲

現時点では、一つの WAV の全体を日本語として認識し、素材の先頭からの発話区間を SRT に出力する。GUI からの実行、対象区間や複数トラックの指定、単語時刻、話者識別、既存字幕と突き合わせた再文字起こしは後続の実装範囲。SRT 読込は現行の取り込み操作で行う。 [製品の受入シナリオ](transcription.feature)
