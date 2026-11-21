# meditor

meditor は、AI がゲーム実況の動画を編集する Windows 専用アプリです。日本語の自動文字起こしを備え、制作者も GUI で編集結果の確認や手動編集ができる製品を目指しています。

## プレビュー版

Rust と Slint で、字幕編集、FFmpeg プレビュー、短 GOP プロキシのスクラブ、ゲーム音と複数話者の WAV のミックスを実装しています。プロジェクトは UTF-8 JSON で保存し、既存の version 1 / 2 と UTF-8 SRT を読み込めます。

AI 動画編集、日本語の自動文字起こし、字幕・動画の書き出し、D3D11VA と WASAPI を用いた高速再生は開発予定です。

## ビルド

### Windows

Rust 1.100.0 の MSVC ツールチェーン、Visual Studio 2022 の C++ ワークロード、CMake、LLVM の libclang、x64 FFmpeg 共有 SDK を用意します。FFmpeg SDK は `include`、`lib`、`bin` を含むものを使います。CMake と MSVC は同梱ビルドする SDL2 に使います。

```powershell
$env:FFMPEG_DIR = 'C:\SDK\ffmpeg-9.1-full_build-shared'
$env:LIBCLANG_PATH = "$env:ProgramFiles\LLVM\bin"
$env:PATH = "$env:FFMPEG_DIR\bin;$env:PATH"
cargo build --manifest-path rust/Cargo.toml --locked --release
.\rust\target\release\meditor.exe
```

### Linux での開発時の検証

Debian 13 系では `build-essential pkg-config libclang-dev libfontconfig-dev libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libsdl2-dev ffmpeg` を導入します。仮想画面での描画テストには `xvfb xauth libxkbcommon-x11-0 fonts-noto-cjk` を使います。Linux ではファイル選択に XDG Desktop Portal が必要です。

```sh
cargo test --manifest-path rust/Cargo.toml --locked
cargo run --manifest-path rust/Cargo.toml --locked --bin meditor
```

## 素材を開く

1. **新規** から元動画、ゲーム音と話者ごとのステレオ 48 kHz PCM s16le WAV、保存先を選びます。
2. **SRT 読込** で字幕を追加し、行を選んで話者と本文を編集します。行をダブルクリックすると開始時刻へ移動します。
3. **素材区間 (秒)** に元動画の開始・終了を入力し、**区間を適用**で一つのクリップとしてプレビューできます。**保存**で編集を確定します。
4. プロキシを生成した場合は **プロキシを選択** で指定します。ドラッグ中だけプロキシを使い、手を離した時点で元動画に戻ります。音声オフセットとゲインはプロジェクト JSON で調整できます。

## 検証

`rust` ディレクトリで受入テストとミューテーションテストを実行できます。行カバレッジの集計範囲、CI の結果、ローカルでの再現手順は [Rust の行カバレッジ](docs/coverage.md) を参照してください。

```powershell
cargo test --locked
cargo mutants --output target --file src/lib.rs --file src/project.rs -- --locked
```

[製品要件](docs/requirements.md) · [編集用語](docs/ubiquitous-language.md) · [初回リリースの機能定義と BDD](docs/features/README.md) · [現行の設計](docs/design.md) · [編集ドメイン](docs/design/editing-domain.md) · [画面と操作](docs/design/editor-screens.md) · [文書の検査](docs/documentation-checks.md) · [受入条件](docs/acceptance.md) · [性能計測](docs/performance.md) · [Rust 採用 ADR](docs/adr/0001-rust-windows-editor.md) · [Rust 版への切り替え ADR](docs/adr/0002-rust-editor-cutover.md)
