# ADR 0001 Rust を Windows エディターの主実装言語にする

## 状態

採用。Rust 版への切り替えは [ADR 0002](0002-rust-editor-cutover.md) に記録した。

## 背景

meditor は AI と制作者が同じプロジェクトを編集する Windows 専用エディターである。初期の C++20 / wxWidgets 版で、字幕10,000件の表示、FFmpeg によるプレビューとプロキシスクラブ、単一クリップの保存まで実装した。移行では、字幕やタイムラインの操作、再生状態、AI が適用する編集操作を、GUI やデコーダーから独立して検証できる構造にしたい。

## 決定

- 新しいエディターの主実装言語に Rust を採用する。字幕とタイムラインのデータ、編集操作、再生制御は安全な Rust で実装する。FFmpeg と Windows API の接続で必要な FFI は、デコード・描画・音声のアダプターに閉じ込める。製品の実行時依存には Qt、QML、Qt Multimedia、Python を含めない。
- GUI の第一候補には Slint を置く。表示範囲だけを実体化する ListView と、D3D11 のプレビュー領域を結合するためのネイティブウィンドウハンドルがある。egui は独自タイムラインを素早く検証する候補、Tauri は WebView2 による UI を採る場合の候補とする。Slint の配布条件と採用ライセンスは配布設計までに確認する。[ListView](https://docs.slint.dev/latest/docs/slint/reference/std-widgets/views/listview/) · [ウィンドウ API](https://docs.slint.dev/latest/docs/rust/slint/struct.Window) · [ライセンス条件](https://slint.dev/terms-and-conditions)
- メディア処理には引き続き FFmpeg を使う。Rust バインディングは現行 CI と同じ SDK で検証し、D3D11VA 固有の処理には `ffmpeg-sys-next` または小さな FFI を使う。Windows 固有の描画・音声には D3D11 と WASAPI を候補とする。[FFmpeg の D3D11VA API](https://ffmpeg.org/doxygen/trunk/hwcontext__d3d11va_8h_source.html) · [WASAPI の再生クロック](https://microsoft.github.io/windows-docs-rs/doc/windows/Win32/Media/Audio/struct.IAudioClock.html)
- PR を再生制御、Windows GUI と映像統合、音声とプロキシの計測に分ける。C++ 版は切り替え前の動作と性能の基準とする。AI 動画編集と文字起こしの位置づけは[製品要件](../requirements.md)に従う。

## 最初の実装と検証

最初の Rust クレートは GUI スレッドでシーク要求を管理する。各要求に連番を割り当て、GUI で提示する直前に最新の未処理要求と照合する。スクラブ中は利用可能ならプロキシを選び、手を離した時点で同じタイムライン上の位置を元動画に要求する。元動画の結果を受理した後で、呼び出し側は再開の指定に従う。停止・素材切り替えでは保留要求を失効させる。ネイティブのデコーダー、レンダラー、音声機器は後続で接続する。

次の試作は Windows 上で Slint の字幕一覧とネイティブ映像領域を同じ画面に置く。字幕10,000件で日本語 IME の変換・確定と候補位置を確認し、同じ PC、素材、操作で C++ 版と比較する。フレーム提示までの p95、字幕編集の応答、再開時の映像・音声差を記録する。GPU 内コピーと CPU への読み戻しは別々に数える。計測点は[性能計測](../performance.md)に従う。これらの実機結果を Slint と高速経路の確定判断に使う。
