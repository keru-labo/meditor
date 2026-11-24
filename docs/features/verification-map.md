# BDD シナリオと検証の対応表

初回リリースの受入仕様にある全シナリオを、実装・仕様モデル・実装テストに対応づける。各シナリオに有限の Quint モデルを対応させ、操作の順序と観測できる結果を固定する。計画中の機能も仕様モデルで扱い、実装状況とテストはそれぞれの欄で確認できる。シナリオアウトラインは1行にまとめ、例ごとに実装や検証の範囲が異なる場合は、その行に記載する。確認の起点は PR #52 が入った `main` の `9193978`。 [機能定義と BDD](README.md) · [確認したコミット](https://github.com/keru-labo/meditor/commit/9193978e6630ebfa3965c32864f0092f0fe8a550)

## 表の読み方

- **実装状況**は、シナリオに書かれた経路と結果を基準にする。**実装済み**はコードで確認できるもの、**一部**は一部の経路や結果を確認できるもの、**計画**はシナリオを定義済みで対応する実装が見つからないもの、**未確認**は根拠が不足するものを指す。`@planned` 付きシナリオの既存部分は **一部（計画）** とする。
- **仕様モデル**は、Quint、Lean、Z3 や独立した Rust の参照モデルが扱う性質と範囲を示す。モデルが同じ領域の一部を扱う場合は、その部分を記す。**実装の検証**は Rust の BDD、受入テスト、単体テスト、MBT、PBT や手動確認手順を示す。関連するテストが一部分を確認する場合は、確認している結果を記す。
- **残っていること**には、現行の BDD を満たすために欠けている実装、モデル、検証を記す。**なし**は該当する根拠が確認できなかったことを指す。
- BDD を変更したら、対応する行を更新し、モデルと実装テストが参照する前提や結果も確認する。仕様変更のレビューでは、利用者に見える結果の変更を BDD に反映する。

## 素材取り込みと音声分離

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [収録方法に応じてトラックを取り込む](media-import.feature#L5) | 一部。元動画と複数の外部 WAV を選択でき、オフセットは JSON で設定できる。 [新規プロジェクト](../../rust/src/main.rs#L229) · [オフセットの設定](../../README.md#素材を開く) | [MediaIntake のシナリオ](../../verification/quint/MediaIntake.qnt) | 一部。元動画・外部トラック参照とオフセットの保存・再読込を検証。 [Rust 受入テスト](../../rust/tests/project_acceptance.rs#L7) | 動画内の別トラック・混合音声をアプリから直接選択し、3例を取り込み操作として検証する。 |
| [混ざったゲーム音と二人の話者をローカルで分離する](media-import.feature#L17) | 計画。現行はあらかじめ別ファイルにした WAV を入力とする。 [現行の素材取り込み](../editor-capability-gap.md#プレビュー版の現状と不足機能) | [MediaIntake のシナリオ](../../verification/quint/MediaIntake.qnt) | なし | ローカル分離、3トラックの登録・個別ミュート、元の混合音声の参照保持を検証する。 |
| [音声分離に失敗した素材をそのまま使う](media-import.feature#L25) | 計画。音声分離の実行経路が未実装。 [現行の素材取り込み](../editor-capability-gap.md#プレビュー版の現状と不足機能) | [MediaIntake のシナリオ](../../verification/quint/MediaIntake.qnt) | なし | 分離失敗理由を表示し、元の混合音声でプレビューを継続できることを検証する。 |

## 日本語の文字起こしと字幕

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [日本語の発話を話者と素材時刻に対応付ける](transcription.feature#L4) | 一部。選択した WAV から日本語の発話区間を SRT に生成し、読込後に字幕の話者名を手動編集できる。 [試用範囲](local-transcription-preview.md#試用範囲) · [字幕の編集](../../rust/src/main.rs#L464) | [Transcription のシナリオ](../../verification/quint/Transcription.qnt) | 一部。模擬 whisper-cli で日本語本文と発話区間の SRT 往復を、履歴テストで手動編集した話者名を検証。 [SRT の統合テスト](../../rust/tests/local_transcription.rs#L50) · [話者名の受入テスト](../../rust/tests/history_acceptance.rs#L172) | 二人の発話を実音声で識別し、単語単位の素材時刻と GUI での話者表示を検証する。 |
| [チャットで指定した範囲を文字起こしする](transcription.feature#L12) | 計画。現行は GUI または CLI で選んだ WAV 1本の全体を処理する。 [試用範囲](local-transcription-preview.md#試用範囲) | [Transcription のシナリオ](../../verification/quint/Transcription.qnt) | なし | チャットから全体・話者・素材区間を単独または組み合わせて指定する4例を検証する。 |
| [編集後のタイムライン時刻から素材を特定する](transcription.feature#L24) | 計画。現行の文字起こしは WAV の全体が対象。 [文字起こし処理](../../rust/src/transcription.rs#L77) | [Transcription のシナリオ](../../verification/quint/Transcription.qnt)。補足検証は 関連のみ。Z3 はプレイヘッドから単一クリップの素材時刻への変換を扱う。 [Z3 の検証範囲](../../verification/z3/README.md#検証対象) | なし | 配置済みクリップでタイムライン 12–14 秒を素材 32–34 秒に解決し、その区間だけが処理されることを検証する。 |
| [再文字起こしで編集済み字幕を差し替えるか選ぶ](transcription.feature#L29) | 一部。SRT 読込時は既存字幕を一括置換する前に確認する。 [SRT 読込](../../rust/src/main.rs#L386) | [Transcription のシナリオ](../../verification/quint/Transcription.qnt) | なし | 再文字起こしの重複検出、選択までの保持、対象話者・区間だけの差し替えを両方の選択肢で検証する。 |

## 字幕の本文・表示区間・スタイルを編集する

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [字幕の本文と表示開始を修正する](subtitle-editing.feature#L4) | 一部 | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt)。補足検証は [Quint・TLC の字幕値の変更](../../verification/quint/EditHistory.qnt#L59) | [ITF の本文変更再生](../../rust/tests/history_acceptance.rs#L557)・[字幕の表示境界](../../rust/src/caption_overlay.rs#L122) · [表示中の本文更新](../../rust/src/caption_overlay.rs#L145) | GUI での表示開始変更を実装し、変更後の4秒・5秒の表示を通しで確認する。 |
| [映像には字幕本文だけを表示する](subtitle-editing.feature#L10) | 実装済み | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | [重なる字幕のプレビュー用本文](../../rust/src/caption_overlay.rs#L122)・[ASS 生成で話者名を付けない](../../rust/src/export.rs#L366) | 2人分を含む焼き込み MP4 を確認し、同じ場面で[字幕一覧の話者表示](../../rust/ui/editor.slint#L360)も照合する。 |
| [既定の字幕は映像の上に背景なしで表示される](subtitle-editing.feature#L16) | 実装済み | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | [矩形なしの ASS 設定](../../rust/src/export.rs#L389)・[既定スタイルの MP4 焼き込み](../../rust/src/export.rs#L516) | 白・青の映像で[プレビューの字幕描画](../../rust/ui/editor.slint#L143)と MP4 を比較し、字幕の背後に元の色が残ることを画素で確認する。 |
| [字幕のフォントと縁取りと背景を反映する](subtitle-editing.feature#L22) | 一部 | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | プロジェクト全体の[フォント・サイズ・縁取り0の ASS 設定](../../rust/src/export.rs#L389)と[焼き込みの色・縁取り画素](../../rust/src/export.rs#L407) | 字幕単位の指定と半透明背景を実装し、対象だけの変更と解除後の見た目を両方の描画経路で確認する。 |
| [チャットで話者の色を変えて個別設定と履歴を確認する](subtitle-editing.feature#L29) | 計画 | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | なし | チャットからの話者別スタイル、字幕単位の上書き、保存・再読込後の Undo を実装し検証する。 |
| [指定のフォントが使えない場合の表示を揃える](subtitle-editing.feature#L37) | 計画 | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | なし | [共通の代替フォント選択と GUI 表示](subtitle-editing.md#確認保存書き出し)を実装し、指定フォントがない Windows 環境で両経路を比較する。 |
| [プロジェクト全体の字幕スタイルを変更する](subtitle-editing.feature#L43) | 実装済み | [SubtitleWorkflow のシナリオ](../../verification/quint/SubtitleWorkflow.qnt) | [設定の一括 Undo・Redo と保存](../../rust/tests/history_acceptance.rs#L16)・[焼き込みの文字色・縁取り画素](../../rust/src/export.rs#L407)・[設定値を直接注入するプレビュー撮影](../../rust/src/main.rs#L963) | 実際の GUI 操作で適用・Undo し、プレビューと MP4 で色・縁取り・背景を照合する。 |

## 映像・タイムライン編集

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [上位の映像を小窓に置き、同じ時刻の音声と確認する](video-timeline.feature#L4) | 一部 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt)。補足検証は 単一映像のシークに限り [Lean の旧フレーム拒否](../../verification/lean/SeekController.lean#L94)・[Z3 の素材位置](../../verification/z3/timeline.smt2#L33) | [単一映像のシーク受入テスト](../../rust/tests/seek_acceptance.rs#L4) · [シーク再開スモーク](../../rust/src/main.rs#L891) · [ゲーム音と話者 WAV のミックス](../../rust/src/audio.rs#L313)・[字幕と音声を含む MP4](../../rust/src/export.rs#L516) | 複数映像トラックの小窓合成を実装し、シーク後の重なり・音声・字幕をプレビューと出力で確認する。 |
| [分割、トリミング、並べ替えをしても元素材を保持する](video-timeline.feature#L12) | 一部 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt)。補足検証は 単一区間の変更のみ [Z3 の素材位置保持・クランプ](../../verification/z3/timeline.smt2#L63)・[Quint・TLC の履歴更新](../../verification/quint/EditHistory.qnt#L142) | [単一区間と字幕の保存](../../rust/tests/project_acceptance.rs#L7)・[区間編集の ITF 再生](../../rust/tests/history_acceptance.rs#L557) | [現行の区間入力](../../rust/src/main.rs#L518)に加え、任意位置への配置、GUI 分割、チャットでの区間修正と並べ替えを実装し、元素材が保持されることを確認する。 |
| [削除で空いた時間の扱いを選ぶ](video-timeline.feature#L20) | 計画 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt) | なし | 空きを保持する削除では同期する音声・字幕は5秒のまま、リップル削除では2秒に移る。両方で対象外の効果音が7秒に残り、一括 Undo で元の配置に戻ることを検証する。 |
| [クリップ境界へ吸着させるか、自由に位置を決める](video-timeline.feature#L33) | 計画 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt) | なし | 複数選択・移動と吸着の切替を実装し、相対間隔と指定位置を確認する。 |
| [再生速度を変えて同じ場面の音声と字幕をそろえる](video-timeline.feature#L40) | 計画 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt) | なし | クリップ速度と結び付いた音声・字幕の時刻変換を実装し、2倍速のプレビューと書き出しを確認する。 |
| [クリップの境界でクロスフェードする](video-timeline.feature#L48) | 計画 | [VideoTimeline のシナリオ](../../verification/quint/VideoTimeline.qnt) | なし | クロスフェードを実装し、素材余裕のある2クリップ間でプレビューと書き出しの遷移時間・映像を確認する。 |

## 音声編集とミックス

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [話者音声の配置とトリミングで有効な区間が変わる](audio-editing.feature#L4) | 一部。トラックをオフセットで配置し、WAV の範囲内を他のトラックとミックスする。 [PCM ミックス](../../rust/src/audio.rs#L44) | [AudioEditing のシナリオ](../../verification/quint/AudioEditing.qnt) | 一部。サンプル単位で複数トラックの正負オフセットと重なりを検証。 [Rust ユニットテスト](../../rust/src/audio.rs#L313) | 音声クリップの個別トリミング、元素材保持、映像のない時間の背景表示と音声継続を検証する。 |
| [音量とフェードをプレビューで確認し、同じ設定で書き出す](audio-editing.feature#L13) | 一部。JSON のトラックゲインをプレビューと書き出しの共通 PCM ミックスに適用する。 [ゲイン適用](../../rust/src/audio.rs#L44) · [書き出しミックス](../../rust/src/audio.rs#L82) | [AudioEditing のシナリオ](../../verification/quint/AudioEditing.qnt) | 一部。独立したトラックゲインを含む PCM 合算と、MP4 内のオフセット付き WAV 出力を検証。 [PCM テスト](../../rust/src/audio.rs#L313) · [MP4 テスト](../../rust/src/export.rs#L517) | クリップ単位の音量、前後のフェード、レベル表示、プレビューと出力の一致を検証する。 |
| [ミュートとソロ試聴で出力を使い分ける](audio-editing.feature#L21) | 計画。ゲイン 0 は無音になるが、ミュートとソロを切り替える操作は未実装。 [ゲイン適用](../../rust/src/audio.rs#L44) · [現行の音声編集](../editor-capability-gap.md#プレビュー版の現状と不足機能) | [AudioEditing のシナリオ](../../verification/quint/AudioEditing.qnt) | なし | ミュート保持、プレビューだけのソロ、ソロ解除、書き出しとの出し分けを検証する。 |
| [チャットで変更した音量を GUI から元に戻す](audio-editing.feature#L30) | 計画。ゲインはプロジェクト JSON の項目で、編集履歴に音量操作がない。 [トラックの設定](../../rust/src/project.rs#L27) · [履歴の編集操作](../../rust/src/history.rs#L24) | [AudioEditing のシナリオ](../../verification/quint/AudioEditing.qnt) | なし | チャットで指定トラックを −6 dB にし、GUI と履歴の表示、Undo と他トラックの設定保持を検証する。 |

## チャットでの AI 編集

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [依頼に応じて編集可能な実況動画を作る](chat-editing.feature#L4) | 一部。字幕を参照した依頼で単一区間を適用し、素材時刻を基準に字幕を表示できる。 [GUI 適用処理](../../rust/src/main.rs#L1142) · [字幕の時刻基準](../../rust/src/main.rs#L545) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 単一区間の応答を反映する条件。 [Quint](../../verification/quint/ChatEdit.qnt#L151) | 一部。区間選択の入出力と単一区間の字幕時刻を検査。 [区間選択](../../rust/src/ai.rs#L233) · [素材時刻の字幕](../../rust/tests/project_acceptance.rs#L72) | 複数の見どころの配置、待機区間の除去・連結後の字幕同期、依頼からプレビュー・修正までの通し検証 |
| [同じチャットで追加指示し、GUI でも修正する](chat-editing.feature#L17) | 一部。GUI で字幕は修正できる。複数クリップの配置と追加指示は不足。 [現状と不足機能](../editor-capability-gap.md#プレビュー版の現状と不足機能) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt) | 一部。字幕修正とプレビューの参照結果を検査。 [Rust 単体テスト](../../rust/src/caption_overlay.rs#L145) | 会話を引き継いだ並べ替えと話者音量の変更、同じプロジェクトでの通し検証 |
| [チャットで映像と音声の処理を追加する](chat-editing.feature#L24) | 一部。チャットで縦横比を出力設定として提案できる。 [AI 応答型](../../rust/src/ai.rs#L24) · [GUI 適用処理](../../rust/src/main.rs#L1159) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt) | 一部。明示された出力設定と未指定値を区別する応答解析の検査。 [Rust 単体テスト](../../rust/src/ai.rs#L300) | チャットからのテロップ装飾・ズーム・効果音と、縦長の設定を同じプロジェクトとプレビューに反映する検証 |
| [中断後に届いた編集結果で作業中の変更を上書きしない](chat-editing.feature#L36) | 実装済み。中断後の応答を破棄し、適用時に字幕などの前提を照合する。 [中断と結果の受け取り](../../rust/src/chat.rs#L47) · [GUI 適用処理](../../rust/src/main.rs#L1117) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 中断→字幕編集→応答のシナリオと編集保持の不変条件。 [Quint・TLC](../../verification/quint/ChatEdit.qnt#L130) | 一部。中断後の遅延応答を Rust に再生し、字幕と要求の状態を照合。入力一致は単体でも検査。 [ITF 再生](../../rust/src/chat.rs#L176) · [入力照合](../../rust/src/chat.rs#L142) | GUI の中断と字幕編集から、遅い応答を取り込むところまでの通し検証 |
| [一度に依頼した編集の途中で失敗した場合は現在の編集を保持する](chat-editing.feature#L43) | 計画。現行の応答は単一区間と任意の出力設定。 [AI 応答型](../../rust/src/ai.rs#L24) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 単一区間の依頼が失敗したときの編集保持。 [Quint・TLC](../../verification/quint/ChatEdit.qnt#L130) | 一部。不正な単一区間応答の拒否を検査。 [Rust 単体テスト](../../rust/src/ai.rs#L277) | カットと音量をまとめて適用する経路と、途中失敗時の両方の保持・理由表示の検証 |

## AI による実況動画の切り出しと GUI での修正

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [編集指示に合う発話を含む区間を AI が選ぶ](ai-clip-editing.feature#L9) | 実装済み（[時刻付き字幕を送って区間を選択](../../rust/src/ai.rs#L63)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L92) は選択済み区間の適用のみ | [Rust 単体](../../rust/src/ai.rs#L233) で送信内容と固定応答の解釈 | [実機手順](../ai-selection.md#書き出しまで確認する) はある。対象発話の全体を含み、別の発話と重ならないことを実際の Codex 応答で確認 |
| [AI が選んだ素材区間が編集可能なクリップになる](ai-clip-editing.feature#L20) | 実装済み（[区間反映と表示](../../rust/src/main.rs#L1142)、[音声再生開始](../../rust/src/main.rs#L865)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L151) で区間の適用と履歴更新 | [Rust 単体](../../rust/src/ai.rs#L259) で区間応答、[音声単体](../../rust/src/audio.rs#L313) でトラックのオフセット | 固定の AI 応答から GUI の適用表示と 10 秒の長さ、指定時刻での映像・音声同期まで確認 |
| [GUI でクリップの開始を調整して保存する](ai-clip-editing.feature#L30) | 実装済み（[区間編集](../../rust/src/main.rs#L518)、[保存](../../rust/src/main.rs#L266)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/EditHistory.qnt#L43) で区間変更の履歴、[Z3](../../verification/z3/README.md#検証対象) で境界変更後の再生時刻 | [Rust 受入](../../rust/tests/project_acceptance.rs#L47) で区間・長さ・音声設定の再読込、[音声単体](../../rust/src/audio.rs#L313) でオフセット | GUI で開始だけを変更し、同期と再読込まで一続きで確認 |
| [GUI でクリップの終了を調整する](ai-clip-editing.feature#L39) | 実装済み（[開始・終了欄から区間を更新](../../rust/src/main.rs#L518)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/EditHistory.qnt#L61) で区間値の変更、[Z3](../../verification/z3/README.md#検証対象) で変更後の再生時刻 | [Rust 受入](../../rust/tests/project_acceptance.rs#L47) で設定区間の長さ | AI が設定した区間の終了だけを GUI で変え、30–38 秒の区間と 8 秒の表示を確認 |
| [無効な素材区間への GUI 編集は修正前のクリップを保持する](ai-clip-editing.feature#L44) | 実装済み（[GUI の更新経路](../../rust/src/main.rs#L518)、[区間の検証](../../rust/src/project.rs#L161)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/EditHistory.qnt#L61) で無効区間を一つの値に抽象化し、変更前の状態を保持 | 開始時刻 `-1` をパーサーで拒否し、開始と終了が同じ区間や、終了が素材を越える区間も拒否する。 [時刻の解析](../../rust/tests/project_acceptance.rs#L182) · [ITF 再生](../../rust/tests/history_acceptance.rs#L557) · [素材終端の受入テスト](../../rust/tests/history_acceptance.rs#L195) | GUI から例表の4通りすべてを入力し、区間が維持されることを確認 |
| [AI が対象区間を選べない理由が分かる](ai-clip-editing.feature#L56) | 実装済み（[字幕がない場合](../../rust/src/main.rs#L1087)、[該当なしの表示](../../rust/src/main.rs#L1138)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt) | [Rust 単体](../../rust/src/ai.rs#L277) で該当なし応答、[入力チェック](../../rust/src/ai.rs#L360) で空の字幕 | 両ケースの GUI 表示と空のタイムラインを確認 |
| [AI が返した無効な素材区間を適用できない](ai-clip-editing.feature#L68) | 実装済み（[AI 応答の区間検証](../../rust/src/ai.rs#L188)、[エラーの表示](../../rust/src/main.rs#L1524)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L92) で失敗後に編集を保持。応答の時刻検証は AiClipWorkflow が扱う | [Rust 単体](../../rust/src/ai.rs#L277) で下限未満・上限超過・長さゼロを拒否 | 負の開始時刻を含む例表の応答を GUI まで通し、表示と空のタイムラインを確認 |
| [AI 処理の失敗で途中の編集操作が反映されない](ai-clip-editing.feature#L81) | 実装済み（[完了状態の検証](../../rust/src/ai/app_server.rs#L528)、[適用前のエラー伝播](../../rust/src/main.rs#L1138)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L148) で失敗応答後に編集を保持 | 関連のみ。実行許可を求める応答をエラーとして扱う。 [模擬 RPC](../../rust/src/ai/app_server.rs#L712) | 途中の応答の後に失敗を返すケースを GUI まで通し、状態保持と表示を確認 |
| [中断した AI 処理が後から結果を返しても適用されない](ai-clip-editing.feature#L87) | 実装済み（[GUI の中断](../../rust/src/main.rs#L1375)、[遅延結果の破棄](../../rust/src/chat.rs#L77)） | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L132) で中断した処理の応答を破棄 | [ITF 再生](../../rust/src/chat.rs#L176) で中断後の有効な結果の破棄と手動字幕の保持を照合 | GUI の中断からタイムライン保持までの結合検証 |
| [処理中に前提が変わったときは制作者の変更を保持する](ai-clip-editing.feature#L93) | 一部。動画の変更は現行では別動画のプロジェクトを開くか作る経路。字幕の差し替えとクリップの追加には応答の破棄がある。 [プロジェクト切り替え](../../rust/src/main.rs#L170) · [字幕差し替え](../../rust/src/main.rs#L1269) · [クリップ更新後の応答破棄](../../rust/src/main.rs#L1130) | [AiClipWorkflow のシナリオ](../../verification/quint/AiClipWorkflow.qnt)。補足検証は [Quint](../../verification/quint/ChatEdit.qnt#L162) で素材・文字起こし・手動クリップの各変更を保持 | [Rust 単体](../../rust/src/chat.rs#L114) で無効化済みの応答を破棄、[条件照合](../../rust/src/chat.rs#L142) で字幕・区間・入力欄を確認 | 同じプロジェクトでの素材動画の差し替えを扱い、GUI で3例それぞれの変更保持と表示を確認 |

## FFmpeg 設定とノード編集

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [FFmpeg の項目を専用画面から選ぶ](ffmpeg-controls.feature#L4) | 一部。現行の[固定の MP4 用設定画面](../../rust/ui/export-settings.slint#L26)は画質・縦横比のみ | [FilterGraph のシナリオ](../../verification/quint/FilterGraph.qnt) | [固定の設定値と既定値を確認](../../rust/src/export.rs#L642) | 配布構成から項目・型・選択肢を列挙し、適用先・フィルター・エンコーダー・出力形式を扱う専用画面 |
| [チャットで指定したフィルターを GUI で確認して調整する](ffmpeg-controls.feature#L11) | 計画 | [FilterGraph のシナリオ](../../verification/quint/FilterGraph.qnt) | なし | 話者別フィルターの入力・保持・GUI 編集と、別の話者に影響しないことの検証 |
| [映像を分岐して拡大し、小窓として合成する](ffmpeg-controls.feature#L18) | 計画 | [FilterGraph のシナリオ](../../verification/quint/FilterGraph.qnt) | なし | 映像ノードの分岐・合成を実装し、ノード画面に両経路と合成順が表示され、プレビューに元映像と拡大した小窓が映ることを検証 |
| [AI が組んだ音声ノードの分岐を GUI で調整する](ffmpeg-controls.feature#L25) | 計画。既存の WAV ミックスは固定経路 | [FilterGraph のシナリオ](../../verification/quint/FilterGraph.qnt) | 関連のみ。 [WAV のゲイン・オフセット付きミックス](../../rust/src/audio.rs#L313) | AI と GUI で同じ音声ノードを編集し、話者 A のみにフィルターを適用して合成する。ゲーム音側の処理が変わらないことも確認 |
| [種類の異なる端子の接続を適用しない](ffmpeg-controls.feature#L31) | 計画 | [FilterGraph のシナリオ](../../verification/quint/FilterGraph.qnt) | なし | 端子の型検査、問題箇所の表示、有効な前構成でプレビューを継続する処理 |

## チャット履歴と作業履歴

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [保存したチャットと作業履歴から編集を再開する](edit-history.feature#L5) | 一部（計画）。Undo / Redo 候補を保存できる。 [プロジェクト保存・読込](../../rust/src/project.rs#L146) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt) | 一部。Undo / Redo 候補を保存・復元する検査。 [Rust 受入テスト](../../rust/tests/history_acceptance.rs#L247) | 前回の会話を復元して続行する機能と、AI・人の操作の時系列表示。その一連の検証 |
| [GUI とチャットで同じ作業履歴を元に戻し、やり直す](edit-history.feature#L13) | 一部（計画）。GUI の Undo / Redo は字幕・区間などに対応する。 [GUI の履歴操作](../../rust/src/main.rs#L612) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 字幕・区間の個別操作と Undo / Redo。 [Quint・TLC](../../verification/quint/EditHistory.qnt#L65) | 一部。字幕と区間を順に戻してやり直す検査。 [Rust 受入テスト](../../rust/tests/history_acceptance.rs#L128) · [ITF 再生](../../rust/tests/history_acceptance.rs#L557) | カットと音量の一括変更、それを GUI とチャットの両方から同じ履歴で戻す検証 |
| [後から行った独立した字幕修正を残してカット変更だけ差し戻す](edit-history.feature#L26) | 計画 | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt) | なし | 独立した後続編集を保持する選択的差し戻しと、対象の変更への参照 |
| [差し戻しが後続の編集に影響する場合は確認を待つ](edit-history.feature#L34) | 計画 | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt) | なし | 依存する字幕への影響提示、確認前の保持、確認・キャンセルの両例の検証 |
| [Undo の後の新しい編集で通常の Redo の候補を消す](edit-history.feature#L47) | 実装済み。有効な新規編集が反映されたときに Redo 候補を消す。 [履歴の実装](../../rust/src/history.rs#L167) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 新規編集で履歴を分岐させ、候補を破棄する。 [Quint・TLC](../../verification/quint/EditHistory.qnt#L142) | あり。Rust BDD、履歴の受入テスト、ITF 再生。 [BDD ランナー](../../rust/tests/history_bdd.rs#L31) · [受入テスト](../../rust/tests/history_acceptance.rs#L180) · [ITF 再生](../../rust/tests/history_acceptance.rs#L557) · [GUI レポートのボタン状態](../../rust/src/history_report.rs#L119) | 実際のクリック入力を含む GUI 操作の確認 |
| [Redo の候補から外れた変更を作業履歴から確認する](edit-history.feature#L54) | 計画 | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は Quint は通常の Redo 候補までを扱う。 [検証範囲](../../verification/quint/README.md#作業履歴の-undo--redo) | なし | Redo 候補から外れた変更を保持・表示する機能とその検証 |
| [保存後に開き直したプロジェクトで Undo と Redo を使う](edit-history.feature#L59) | 実装済み。履歴を復元し、GUI から操作できる。 [保存・読込](../../rust/src/project.rs#L146) · [GUI の履歴操作](../../rust/src/main.rs#L612) | [ChatHistoryWorkflow のシナリオ](../../verification/quint/ChatHistoryWorkflow.qnt)。補足検証は 正常な保存・再読込と、両履歴を持った状態での Redo。 [Quint・TLC](../../verification/quint/EditHistory.qnt#L86) | あり。保存後の未保存編集を挟む ITF、Rust BDD、Proptest、両ボタンと復元後の区間を確認する GUI レポート。 [ITF](../../verification/quint/traces/edit_history_reopen.itf.json) · [BDD ランナー](../../rust/tests/history_bdd.rs#L31) · [保存・再読込の受入テストと Proptest](../../rust/tests/history_acceptance.rs) · [GUI レポート](../../rust/src/history_report.rs#L162) | ネイティブのファイル選択とクリック入力を含む GUI 操作の確認 |

## 字幕を焼き込んだ動画の書き出し

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [チャットで依頼し字幕焼き込み済みの動画まで作る](video-export.feature#L4) | 一部。AI が単一区間と設定を提案し、保存先の確定後に焼き込み MP4 を出力し、パスを表示 | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt)。補足検証は Quint [AI 結果の反映](../../verification/quint/ChatEdit.qnt)。出力は対象外 | [MP4 の焼き込み・音声の統合テスト](../../rust/src/export.rs#L517) | AI 依頼だけで既定の保存先に出力する経路、複数区間のまとめ・待ち時間除去、チャットからの一貫テスト |
| [確認後の書き出しで人が修正した内容を使う](video-export.feature#L18) | 一部。区間と修正した字幕を確認し、保存先確定時に編集内容を取得 | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt)。補足検証は Quint [AI 結果の反映](../../verification/quint/ChatEdit.qnt)。承認と音声設定は対象外 | [字幕・音声のスナップショットと実 MP4](../../rust/src/export.rs#L517) | GUI からの話者 B のミュートと、修正・承認後の字幕および音声を確認する一連のテスト |
| [今回の書き出し設定をチャットで上書きする](video-export.feature#L27) | 一部。AI は画質と縦横比を提案でき、保存先と名前は対話で選ぶ | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | [応答の設定項目の検証](../../rust/src/ai.rs#L300)、[提案から出力設定への変換](../../rust/src/export.rs#L652) | 保存先・名前のチャット指定、設定した既定値の保持と依頼単位での上書き |
| [書き出しに失敗した後も編集を続ける](video-export.feature#L33) | 実装済み。 [失敗理由を表示して書き出し状態を解除](../../rust/src/main.rs#L1040) | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | [既存ファイルへの出力拒否と保持](../../rust/src/export.rs#L629) | 書き込み不能な保存先で理由表示、編集内容保持、別の保存先での再試行を GUI で確認 |
| [書き出しを始めた後の編集は次の出力に反映される](video-export.feature#L39) | 実装済み。 [書き出し開始時のスナップショットをワーカーに渡す](../../rust/src/main.rs#L1021) | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | [取得後に元の字幕・音声を消しても MP4 に反映されることを確認](../../rust/src/export.rs#L563) | GUI で実行中に字幕を変更し、現在の MP4 と次の MP4 を照合 |
| [手動書き出しで縦長・高画質と黒い余白を選ぶ](video-export.feature#L45) | 一部。画質と縦長を選択でき、常に表示比を保って黒で埋める | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | [実 MP4 の 720 × 1280、黒帯、表示比、高画質 CRF を確認](../../rust/src/export.rs#L675) | 「全体を表示」と黒い背景を明示的に選ぶ UI |
| [縦長の枠を映像で満たして中心位置を調整する](video-export.feature#L51) | 計画。現行は[全体が収まるように縮小して余白を追加](../../rust/src/export.rs#L86) | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | なし | 枠を満たす切り抜きと中心位置の調整、プレビューと MP4 の構図の照合 |
| [映像の全体を残し余白を指定した色で埋める](video-export.feature#L57) | 一部。MP4 出力では映像全体を収め、余白を黒で埋める。 [書き出しフィルター](../../rust/src/export.rs#L202) | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | [縦長 MP4 の表示比と黒い余白](../../rust/src/export.rs#L675) | 出力キャンバスでのプレビューと余白の色指定を実装し、プレビューと MP4 の両方で青が余白だけに適用され、映像全体と字幕背景が保持されることを確認 |
| [AI が選んだ区間と設定を確認してから書き出す](video-export.feature#L63) | 実装済み。 [提案を区間に反映](../../rust/src/main.rs#L1117)し、GUI で設定を変更して保存先を確定 | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt)。補足検証は Quint [AI 結果の反映](../../verification/quint/ChatEdit.qnt)。字幕と出力は対象外 | [字幕の区間変換](../../rust/src/export.rs#L474)、[提案値の変換](../../rust/src/export.rs#L652)、[焼き込み MP4](../../rust/src/export.rs#L517) · [高画質での手動確認手順](../ai-selection.md#書き出しまで確認する) | GUI で高画質を標準に変更し、720 × 1280 の MP4 に標準のエンコード設定が適用され、素材の8–11秒と修正した字幕が出力されることを確認。「準備開始」がプレビューにも出力にも表示されないことを照合 |
| [AI の提案を確認して書き出しを中止する](video-export.feature#L76) | 実装済み。 [設定画面のキャンセルは出力を起動しない](../../rust/ui/export-settings.slint#L42)。 [通常と AI で別の設定画面を使用し、通常画面の初期値を保持](../../rust/ui/editor.slint#L404) | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt) | 一部。 [高画質・縦長と両設定未指定の変換](../../rust/src/export.rs#L652) · [高画質・縦長でキャンセルする手動手順](../ai-selection.md#書き出しまで確認する) | 縦長のみの提案で標準画質を確認し、キャンセル後のファイル不生成、区間保持、初回の通常画面の既定値を照合 |
| [提案したクリップを取り消すかプロジェクトを切り替える](video-export.feature#L85) | 実装済み。 [区間を元に戻した場合](../../rust/src/main.rs#L501)と[プロジェクトを切り替えた場合](../../rust/src/main.rs#L169)に提案ボタンの状態を解除 | [ExportWorkflow のシナリオ](../../verification/quint/ExportWorkflow.qnt)。補足検証は  [AI 結果の反映](../../verification/quint/ChatEdit.qnt)と[クリップの Undo](../../verification/quint/EditHistory.qnt#L150)。提案ボタンは対象外 | [区間と字幕の基準の Undo / Redo](../../rust/tests/history_acceptance.rs#L293) | 両操作の後で提案ボタンが消えることを GUI で確認 |

## Codex App Server への接続と素材の送信許可

| BDD シナリオ | 実装状況 | 仕様モデル | 実装の検証 | 残っていること |
| --- | --- | --- | --- | --- |
| [ChatGPT にログインした Codex で指示と字幕を送る](ai-connections.feature#L4) | 実装済み（[送信内容の組み立て](../../rust/src/ai.rs#L75)、[認証確認](../../rust/src/ai/app_server.rs#L428)） | [CodexConnection のシナリオ](../../verification/quint/CodexConnection.qnt) | 指示・区間条件・時刻付き字幕だけの JSON を照合し、映像・音声・ローカル素材パスを送信項目に含めないことを確認。模擬 RPC ではテキスト入力だけを送ることを検証。 [JSON の単体テスト](../../rust/src/ai.rs#L245) · [模擬 RPC](../../rust/src/ai/app_server.rs#L649) | [実機手順](../ai-selection.md#windows-で試す) に沿ったログイン済み Windows での確認 |
| [ログインしていない状態で編集を依頼する](ai-connections.feature#L11) | 実装済み（[ChatGPT アカウントが確認できなければログインを案内](../../rust/src/ai/app_server.rs#L428)） | [CodexConnection のシナリオ](../../verification/quint/CodexConnection.qnt) | [模擬 RPC](../../rust/src/ai/app_server.rs#L659) で非 ChatGPT アカウントの場合に指示と字幕を送らないことを確認 | 未ログイン応答・GUI の案内・編集状態の保持を一続きで確認 |
| [実行ツールの無効化または区間選択モデルを確認できない](ai-connections.feature#L18) | 実装済み（[機能設定の確認](../../rust/src/ai/app_server.rs#L419)、[指定モデルの照合](../../rust/src/ai/app_server.rs#L449)） | [CodexConnection のシナリオ](../../verification/quint/CodexConnection.qnt) | [模擬 RPC](../../rust/src/ai/app_server.rs#L659) で Code Mode 有効・他モデルの場合に指示と字幕を送らないことを確認 | GUI のエラー表示と編集状態の保持を確認 |
| [文字起こし・話者識別と音声分離を PC 内で行う](ai-connections.feature#L26) | 一部（[ローカル文字起こし](../../rust/src/transcription.rs#L110)） | [CodexConnection のシナリオ](../../verification/quint/CodexConnection.qnt) | [模擬 CLI の受入](../../rust/tests/local_transcription.rs#L50) でローカル WAV からの SRT 生成と入力保持 | 話者識別と音声分離の実装、全経路で映像・音声を送信しないことの確認 |
