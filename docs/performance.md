# 元動画とプロキシの比較

## フィクスチャ

FFmpeg CLI（libx264 エンコーダー付き）を PATH に置きます。字幕10,000件、60秒の 720p / 30fps の元動画、480px 幅・GOP 6 のプロキシ、ゲーム音と話者相当の音声2本を生成します。音声は周波数の異なる合成音です。

```powershell
cargo run --manifest-path rust/Cargo.toml --locked --release --bin meditor-fixture -- artifacts/fixture
cargo run --manifest-path rust/Cargo.toml --locked --release --bin meditor -- artifacts/fixture/fixture.json
```

実況素材を使うときは、初回の比較をゼロ始まりの CFR 動画にそろえ、元動画からプロキシを作ります。フレームレートを変更せず、同じ映像区間で生成します。

```sh
ffmpeg -i original.mp4 -map 0:v:0 -an -vf scale=480:-2 -c:v libx264 \
  -preset veryfast -g 6 -keyint_min 6 -sc_threshold 0 -bf 0 proxy.mp4
ffmpeg -i recording.mkv -map 0:a:0 -ac 2 -ar 48000 -c:a pcm_s16le game.wav
```

音声抽出時はトラックごとに `-map` を変更し、元動画に対する開始時刻差をプロジェクトの `offset_us` に反映します。

## 実行方法

1. 同じ Windows PC で CPU、GPU、ドライバー、OS、表示周波数、オーディオデバイス、FFmpeg の版、素材ハッシュ、ウィンドウサイズを記録します。
2. 字幕全件を読み込み、5秒付近から再生します。**計測開始** を選び、プロキシをオフにしてスライダーを 5→45→15→35 秒の順にドラッグします。各端で0.5秒ほど止め、35秒で手を離します。同じ開始点に戻す操作を含め、マウス入力の時刻と座標を録画またはマクロで記録し、もう一方でも再生します。字幕一覧の先頭・中間・末尾で本文と話者を変更します。**計測保存** で JSON を保存します。
3. 新しいプロセスでプロキシをオンにして同じ操作を行い、別ファイルに保存します。元動画・プロキシを1組として3組以上測ります。ウォームアップを毎回同じ条件で行い、測定時の操作間隔もそろえます。
4. `tools/Summarize-Preview.ps1` に結果を渡します。p95 は昇順の nearest-rank です。シーク入力数、描画数、対象との差はあわせて確認します。高速な連続入力では上書き件数が増えるため、描画できたサンプルの p95 だけで優劣を判断しません。

```powershell
& .\tools\Summarize-Preview.ps1 artifacts\original.json,artifacts\proxy.json
```

## 測定点

- Rust 版の `scrub_frame_submit` は、シーク入力から Rust が Slint にフレームを渡すまで。C++ 版の `scrub_paint` は GUI の描画コールバックまで。それぞれ集計し、両版で実表示 p95 を比較する際は、同じタイムコード映像と Windows のプレゼンテーション記録または高速度撮影を使う。
- Rust 版の `resume_frame_submit` は映像 PTS と音声の再開要求時刻の差。C++ 版の `resume_paint` は映像 PTS と SDL の消費ベースの音声時刻の差。物理 A/V 差は、同じ既知時刻の映像フラッシュと音パルスを使い、画面提示の記録と WASAPI ループバックを同一時刻軸で照合する。
- `caption_edit_dispatch` は GUI が入力を受けてからモデル更新と行の再描画要求まで。キー入力から文字表示までの遅れは OS 計測または高速度撮影で補う。`gui_timer_interval` は UI イベントループの間隔で、p95 と最大を記録する。

`subtitle-edit-bay` から、10,000件のフィクスチャ、同一 PC での比較、独立プロセスでの反復、イベントループとプレイヘッドの計測方法を参考にした。既存実装の数値を用いる場合は同じ操作と観測点にそろえる。 [参照した計測手法](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/docs/GUI_PERFORMANCE.md)

**Windows 実機では未計測**。実表示 p95、元動画とプロキシの差、物理 A/V ずれ、IME と10,000件の UI 応答、GPU テクスチャの受け渡しと CPU 読み戻しは未確認。
