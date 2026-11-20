# 再生基盤と性能計測

## 参照実装

`subtitle-edit-bay` の `0c7db4d44027719bcb5d6500a370aec1da1b09f8` を参照した。

- プレビューパネルは外部から MediaPlayer を受け取り VideoOutput に接続する。タイムラインはビューポート内の字幕と目盛りだけを作る。meditor ではプレーヤーの寿命を画面から分離し、字幕モデルを導入する段階で表示区間のインデックスを用意する。 [プレビュー](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/WorkspacePreviewPanel.qml) · [タイムライン](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/SubtitleTimeline.qml)
- ミキサー画面は映像の MediaPlayer の位置から各音声のオフセットを引き、個別の MediaPlayer と AudioBufferOutput を使う。リアルタイムミキサーは PCM を共通フォーマットにそろえ、各トラックの時刻に合わせて混合し、プリロール後に QAudioSink へ送る。今後の比較では、トラックごとの開始時刻、サンプルの空白、ミュート、シーク時のバッファ破棄を同じ素材で確認する。 [QML の接続](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/AudioMixerPreviewSession.qml) · [PCM ミキサー](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/realtime_audio_mixer.py)
- 既存ハーネスは 3,000 件と 10,000 件の fixture、10 ms タイマーのイベントループ遅延、UI の再生位置追従、最初の映像フレーム、状態遷移、RSS を記録する。入力の生成条件、反復ごとのプロセス分離、同じランナー内での逐次比較、比較対象 SHA と環境の記録を踏襲する。既存の QML 操作ハーネスは既存アプリのオブジェクト名と Python バックエンドに依存するため、meditor の操作アダプターは別途実装する。 [計測条件](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/docs/GUI_PERFORMANCE.md) · [計測スクリプト](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/scripts/run_gui_performance.py) · [比較スクリプト](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/scripts/compare_gui_performance.py)

## 初回の実装

QML は C++ の PlaybackBackend を通してローカル動画を操作する。Qt Multimedia アダプターが QMediaPlayer と QAudioOutput を所有し、プレビューの映像を QVideoSink へ渡す。この経路は Windows、macOS、Linux の基準経路となる。実行時に Python は不要。

シーク要求は16 ms のタイマーでまとめ、Qt に渡す前の古い要求を破棄する。ドラッグ終了で即時送出する。Qt が受理済みのデコード作業や出力を要求 ID で取り消す機構は Qt Multimedia の公開 API にないため、送出後に届く古いフレームや音声の抑止はこの段階では保証できない。モデルの Lean 証明は保留スロットの性質を対象とする。

## 後続 PR の順序

1. 単一動画と連続シークを Windows ネイティブ経路で試す。FFmpeg D3D11VA と描画側のデバイス、アダプター、テクスチャ形式、バインド条件、寿命と同期を確認する。QVideoSink は基準経路の表示接続として使い、ネイティブ経路の描画境界はテクスチャ検証に合わせて定める。同一デバイスでの GPU 内コピーと、別デバイスでの共有を区別する。GPU トレースで staging texture と CPU readback の有無を確認する。初期化や形式の非対応時は Qt 経路へ戻す。
2. PCM を共通レートのタイムラインに配置し、WASAPI 出力の再生クロックに映像を同期する。素材切替とシークで世代を進め、古い世代のデコード結果とミックス済みバッファを破棄する。既にデバイスへ送った音声は停止・リセットで除去できる範囲を調べ、再開のプリロールと実測のずれを記録する。
3. 元動画と同じタイムラインに対応する低解像度・短 GOP プロキシを用意する。時刻原点、ストリームの time base、可変フレームレートを考慮し、フィクスチャで対応表を検証する。スクラブ終了後は同じタイムライン時刻で元動画を開き直し、対象フレームと音声の準備が整ってから再開する。

## Windows 比較手順

1. 同じ PC で、電源設定、GPU とドライバー、Qt、FFmpeg、Windows ビルド、表示リフレッシュレート、音声デバイスとバッファ設定を固定して記録する。同じ実況素材と音声トラックの時刻・レベルを使い、素材ハッシュとプロキシ生成条件を残す。公開用には時刻が既知の映像マーカーと同期音を含む合成素材も使う。
2. Qt Multimedia の元動画、Windows 高速経路の元動画、同じ高速経路のプロキシの順に、同じシーク操作列を各 3 回以上、新しいプロセスで順次実行する。単発、同方向への連続ドラッグ、反転ドラッグ、手を離して再生する操作を含め、要求 ID、目標時刻、入力イベント時刻、最終送出時刻を単調時計で記録する。10,000 件シナリオは字幕モデル導入後に同じ字幕データとビューポートで行う。
3. 対象のタイムライン時刻を含むフレームを PTS と表示時間から特定し、そのフレームを含む描画の表示時刻を取得する。シーク入力から表示までの p50・p95・最大値、欠測、途中で上書きした要求数、古いフレームの表示数を出す。Qt の videoFrameChanged はフレーム到着の補助指標として別に記録する。Qt の描画完了と Windows の実表示を区別し、経路間では同じ観測点で比較する。
4. 再生再開後の A/V ずれを共通タイムラインと単調時計で記録し、同期マーカーの表示時刻と音声ループバックの到達時刻で照合する。10 ms の UI タイマーを使って停止時間の p95・最大値を記録し、プレイヘッド更新を映像時刻と比較する。10,000 件では実体化した字幕 delegate 数も記録する。

**Windows 未計測**。D3D11VA/D3D11 のテクスチャ共有と読み戻し、WASAPI のミックスと同期、プロキシ切替、シーク表示 p95、再開時の A/V ずれ、10,000 件での UI 停止とプレイヘッド遅延は、この基盤の後に検証する。WhisperX と書き出しは別段階で扱う。
