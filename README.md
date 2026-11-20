# meditor

meditor は、AI がゲーム実況の動画を編集する Windows 専用アプリです。日本語の自動文字起こしを備え、制作者も GUI で編集結果の確認や手動編集ができる製品を目指しています。再生・映像処理には FFmpeg を使います。

## 最初のプレビュー版

- 日本語の字幕と話者を、ネイティブのテキスト入力欄から編集できます。一覧は表示行だけを取得し、編集中は対象の1行を更新します。
- 元動画をプレビューし、ドラッグ中だけ短 GOP プロキシに切り替えられます。再生中にドラッグすると、手を離した位置から元動画とミックス音声で再開します。
- プロジェクトは UTF-8 JSON で保存します。既存の UTF-8 SRT を読み込めます。

現在の動画経路は FFmpeg のソフトウェアデコードです。AI 動画編集、日本語の自動文字起こし、字幕・動画の書き出し、D3D11VA と WASAPI を用いた Windows の高速再生経路は開発予定です。

## ビルド

C++20 コンパイラ、CMake 3.24 以降、wxWidgets 3.2 以降、FFmpeg（libavformat、libavcodec、libavutil、libswscale）、SDL2、nlohmann-json が必要です。FFmpeg CLI は素材準備とテストで使います。

### Windows

Visual Studio 2022 の C++ ワークロードと bootstrap 済みの vcpkg を用意し、`VCPKG_ROOT` を設定します。Developer PowerShell で実行してください。

```powershell
cmake -S . -B build -A x64 "-DCMAKE_TOOLCHAIN_FILE=$env:VCPKG_ROOT/scripts/buildsystems/vcpkg.cmake" -DVCPKG_TARGET_TRIPLET=x64-windows
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
.\build\Release\meditor.exe
```

### Linux での開発時の検証

Debian 13 系では `cmake g++ make pkg-config libwxgtk3.2-dev libavformat-dev libavcodec-dev libavutil-dev libswscale-dev libsdl2-dev nlohmann-json3-dev ffmpeg` を導入します。

```sh
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DMEDITOR_TEST_MEDIA=ON
cmake --build build --parallel
ctest --test-dir build --output-on-failure
./build/meditor
```

Windows CI でビルドと機能テストを確認しています。Linux CI は開発時の回帰検出に使い、Windows 実機での性能計測は今後の検証項目です。

## 素材を開く

1. **ファイル → 新規プロジェクト** から元動画、ゲーム音と話者の WAV、保存先を選びます。音声は各トラックをステレオ 48 kHz PCM s16le に準備します。動画内の音声は WAV として抽出して追加します。
2. **字幕を SRT から読み込む** で字幕を追加し、行を選んで話者と本文を編集します。行をダブルクリックすると開始時刻へ移動します。
3. プロキシを生成した場合は **スクラブ用プロキシを選択** で指定します。プロジェクトの音声オフセットとゲインは JSON で調整できます。

合成動画、ゲーム音、話者2本の音声、字幕10,000件を使う手順は [性能計測](docs/performance.md) を参照してください。 [製品要件（レビュー案）](docs/requirements.md) · [設計・保存形式・配布](docs/design.md) · [受入条件](docs/acceptance.md)
