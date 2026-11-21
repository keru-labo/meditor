# OpenAI の単一クリップ選択

Rust の `meditor_playback::ai::select_openai_clip` は、指示に沿った素材区間を一つ選ぶ。OpenAI Responses API の GPT-5.4 mini スナップショット `gpt-5.4-mini-2026-03-17` を指定し、Structured Outputs で整数マイクロ秒の開始・終了、または該当なしを受け取る。 [OpenAI のモデル仕様](https://developers.openai.com/api/docs/models/gpt-5.4-mini) · [Structured Outputs](https://developers.openai.com/api/docs/guides/structured-outputs)

呼び出し元は制作者が送信を選んだときに限り、API キー、プロンプト、現在の再生範囲とそのタイムライン上の字幕を渡す。処理は同期なので、GUI ではワーカーから呼び出す。キーは API の Authorization ヘッダーに使用する。送信する編集データは、プロンプト、範囲の時刻と、その範囲にある字幕の本文・話者ラベル・時刻。素材ファイルを読み込む処理はなく、リクエストに素材ファイルやそのパスを指定する項目はない。API には `store: false` を指定する。

SRT などから得た既存字幕を、素材を選ぶための発話の手掛かりとして使用する。現在の再生範囲の先頭を字幕の 0 秒とし、素材時刻に換算する。たとえば再生範囲が素材の 30–40 秒なら、字幕の 2–4 秒は素材の 32–34 秒となる。範囲に一部重なる字幕は時刻を範囲内に収め、範囲外の字幕と空の字幕は送信対象から除く。

結果は呼び出し元が受け取って適用する。該当なしは `Ok(None)` で表す。構造が不正な応答、未完了の応答、開始と終了が逆転した区間、範囲外の区間はエラーになる。通信には 60 秒のタイムアウトと応答サイズ上限がある。送信する JSON が 1 MiB を超える場合は、対象範囲を短くして再依頼する。

このモジュールには GUI や編集履歴を持たせず、呼び出し元で中断や素材・字幕の変更を検出してから結果を適用する。テストでは合成字幕と模擬 API 応答を使う。 [AI 切り出しの機能定義](features/ai-clip-editing.md) · [外部サービスの製品仕様](features/ai-connections.md)
