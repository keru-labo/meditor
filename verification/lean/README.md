# Lean によるシーク制御の検証

Lean 4 を使い、GUI スレッドの `SeekController` の状態遷移を検証する。モデルは [Rust のコントローラー](../../rust/src/lib.rs) の `request`、`invalidate`、`accept_frame` に対応し、要求の一致判定にシリアル番号、時刻、モードを使う。 [Lean モデル](SeekController.lean)

## 検証する性質

- 初期状態から任意のイベント列を処理したとき、保留中の要求があれば、そのシリアル番号は最後に発行した要求の番号と一致する。
- 新しい要求が発行された後、任意の追加イベントを処理しても、古い要求のフレームを受理しない。時刻とモードが同じ要求も対象とする。
- 保留を解除するとフレームを受理できなくなる。フレームを受理した時点で保留が消えるので、直後の重複通知も受理しない。

## モデルの前提

Lean の自然数でシリアル番号を表し、Rust の `checked_add` が成功する範囲の遷移に対応させる。一つのコントローラーに対する GUI スレッド上のイベント順序を仮定する。証明の適用には Rust とモデルの対応をレビューで維持する必要がある。Rust の実際の結果受理と再生経路の確認には既存のテストを使う。FFmpeg、ワーカーの処理順序、描画、音声の同期は個別のテストと計測の対象とする。 [Rust の受入テスト](../../rust/tests/seek_acceptance.rs)

## 実行方法

Elan で `verification/lean/lean-toolchain` に指定した Lean 4.19.0 を使う。検証には Lean の標準ライブラリーを使う。

```sh
cd verification/lean
lean -DwarningAsError=true SeekController.lean
```

GitHub Actions でも同じ版を使い、Linux の配布アーカイブを SHA-256 で検証する。更新時はツールチェーンの指定と配布アーカイブのハッシュを揃えて変更する。 [Lean の CI](../../.github/workflows/formal-lean.yml)
