# プレビュー版の現状と不足機能

確認対象は単一クリップの AI 書き出し提案までを含む `main`。初回リリースで利用者に提供する動作と受入条件は機能定義に記載する。 [確認したコミット](https://github.com/keru-labo/meditor/commit/1f01b88e52a4a9754bf6218530d3612248632fbc) · [機能定義と BDD](features/README.md)

| 領域 | プレビュー版で使える操作 | 初回リリースに向けて不足している操作 |
| --- | --- | --- |
| 素材取り込み | 元動画 1 本、変換済み WAV、SRT、手動指定のプロキシ | 素材の追加、動画内の音声の選択、音声の形式変換と分離。 [素材取り込み](features/media-import.md) |
| 映像・タイムライン | 単一区間の開始・終了入力、シーク、スクラブ | 複数クリップとトラック、分割・トリミング・移動・リップル編集、映像や音声と字幕の同期。 [タイムライン](features/video-timeline.md) |
| 映像の見せ方 | 元の映像をプレビュー | 複数の映像を重ねる、小窓、トランジション、速度、ズーム・クロップ、処理設定とノード編集。 [タイムライン](features/video-timeline.md) · [FFmpeg 設定](features/ffmpeg-controls.md) |
| 音声 | WAV のミックス、プロジェクトファイルでオフセットとゲインを設定 | 音声クリップの編集、GUI とチャットでの音量・ミュート・フェード、ソロ試聴、レベル確認。 [音声編集](features/audio-editing.md) |
| 文字起こし | ローカルの whisper-cli と用意した WAV から SRT を作り、GUI で読み込む | 素材からの直接実行、単語時刻と話者識別、範囲・トラックを指定した再処理。 [文字起こし](features/transcription.md) |
| 字幕 | 本文・話者の変更、映像への重ね表示と焼き込み | 字幕を追加・分割・結合して表示時刻を変更する操作、フォント・色・縁取り・背景・配置、プレビューと出力の見た目の一致。 [字幕編集](features/subtitle-editing.md) |
| 映像キャンバス・出力 | 画質と縦横比を選び、単一クリップを MP4 に出力 | 書き出しサイズでのプレビュー、枠に合わせた切り出しと位置調整、余白の背景設定、出力設定の保存と依頼ごとの指定。 [書き出し](features/video-export.md) |
| AI 編集・履歴 | Codex が単一区間と出力設定を提案し、人が確認後に出力。字幕・クリップの Undo / Redo を保存 | 会話を続けて複数の編集を適用する操作、履歴から指定した変更の差し戻し、依頼から出力までの実行。 [AI 編集](features/chat-editing.md) · [履歴](features/edit-history.md) |

字幕を表示する黒い矩形と、縦長などに変換したとき映像の外側へ書き出される黒い余白は別の問題。現状はプレビューの字幕に半透明の暗い矩形があり、焼き込み MP4 には固定のフォント・縁取りが使われる。指定の縦横比へ変換するときは映像全体を残し、余った部分を黒で埋める。 [字幕プレビューの実装](https://github.com/keru-labo/meditor/blob/1f01b88e52a4a9754bf6218530d3612248632fbc/rust/ui/editor.slint#L109-L145) · [字幕と出力キャンバスの実装](https://github.com/keru-labo/meditor/blob/1f01b88e52a4a9754bf6218530d3612248632fbc/rust/src/export.rs)

## 字幕機能の参照

- `subtitle-edit-bay` には、字幕ごとのフォントとサイズ調整、話者ごとの文字色、共有の基本サイズ・縁取りの色と太さがある。 [字幕設定](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/SubtitleModeSettings.qml#L329-L385) · [縁取りと文字サイズ](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/AdvancedSettingsPopup.qml#L213-L289)
- プレビューは色付きの文字と縁取りを映像上に描く。書き出しも矩形の字幕背景を付けない設定を使う。 [プレビュー](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ui/components/SubtitleOverlay.qml#L115-L154) · [焼き込み時のスタイル](https://github.com/keru-labo/subtitle-edit-bay/blob/0c7db4d44027719bcb5d6500a370aec1da1b09f8/src/ass_template.py#L7-L20)

meditor の機能定義では、背景なしを既定にしたうえで、希望する場合に背景色と不透明度を付けられるようにした。文字色などの設定は全体・話者・個別字幕ごとに扱い、プレビューと出力で揃える。 [字幕スタイル](features/subtitle-editing.md#スタイルの編集)
