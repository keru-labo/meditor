# ローカル文字起こしの試用

既存のエディターに読み込める UTF-8 SRT を、選択した音声ファイルから PC 内で生成する。自分で用意した `whisper-cli` と日本語に対応する多言語モデルを使用する。CLI の接続仕様は whisper.cpp v1.8.3 のフラグに合わせている。 [whisper.cpp v1.8.3](https://github.com/ggml-org/whisper.cpp/releases/tag/v1.8.3) · [製品の文字起こし仕様](transcription.md)

## エディターから試す

1. 編集画面の **文字起こし** を押す。
2. 自分で用意した `whisper-cli.exe`、日本語に対応する多言語モデル、モノラル 16 kHz PCM s16le の WAV、SRT の保存先を順に選ぶ。
3. 完了すると保存先が表示される。時刻が対応するプロジェクトで **SRT 読込**から取り込む。既存字幕がある場合は、置き換えの確認が表示される。

文字起こし中も編集できる。**文字起こしを中止**を押すと処理を終了し、途中の SRT を破棄する。処理の完了または中止後にエディターを閉じられる。SRT を生成した時点ではプロジェクトは変更されないため、取り込む前に対象プロジェクトと時刻の対応を確認できる。

## CLI で試す

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

現時点では、一つの WAV の全体を日本語として認識し、素材の先頭からの発話区間を SRT に出力する。対象区間や複数トラックの指定、単語時刻、話者識別、既存字幕と突き合わせた再文字起こしは後続の実装範囲。SRT 読込は現行の取り込み操作で行う。 [製品の受入シナリオ](transcription.feature)
