# Rust の検証ツール評価

## 採用する範囲

- 入力の組み合わせを列挙できる場合は、通常の Cargo テストで性質を検査する。字幕の保存・読み込みでは、開始時刻の異なる字幕と同時刻の字幕を含む3件を全6通りの順序で保存する。読み込み後の開始時刻順、同時刻の保存時の順序、字幕 ID・話者・本文・区間の保持を検査する。 [受入テスト](../rust/tests/project_acceptance.rs)
- 既存の cargo-mutants は、対象を絞ったテストの点検に使う。テストに失敗した変異を検出、ビルドできない変異を実行不能、テストを通過した変異を未検出として読む。変異が検出できても、実際の実装について得られる証拠はその変異と入力の範囲に限られる。 [結果の読み方](https://mutants.rs/using-results.html)

## 比較した候補

| 候補 | 判断 |
| --- | --- |
| Proptest | 入力を生成し、失敗した入力を縮小できる。字幕の今回の性質は6通りを直接検査する。将来、字幕パーサーの有効・無効入力や多様な編集操作の列を検査する場合に生成と縮小が役立つ。 [Proptest の説明](https://proptest-rs.github.io/proptest/) |
| Miri | Rust コードを実行して未定義動作を検出する。nightly が必要で、ネイティブ関数の呼び出しは実験的な対応。meditor では FFI から独立した Rust のコードに適用する候補。FFmpeg、SDL2、デバイス連携の確認は統合テストと実機検証で扱う。 [Miri の README](https://github.com/rust-lang/miri/blob/master/README.md) |
| Kani | Rust の実装を証明ハーネスからモデル検査できる。対応する開発環境とループなどの探索境界を選ぶ必要がある。長さや整数演算だけで記述できる、FFI を持たない小さな境界が候補。 [Kani の導入環境](https://model-checking.github.io/kani/install-guide.html) · [Kani の利用手順](https://model-checking.github.io/kani/usage.html) |

この評価で実行したのは Cargo の受入テストと cargo-mutants。Proptest、Miri、Kani の試行と CI 組み込みは評価の次段階とする。

## 再現手順

[meditor のビルド手順](../README.md#ビルド)に従って依存ライブラリを用意し、リポジトリの `rust` ディレクトリで実行する。

```sh
cargo install --locked cargo-mutants --version 27.1.0
cargo test --locked --release --test project_acceptance
cargo mutants --profile release --timeout 120 --output target --file src/project.rs --re 'Project::load|Project::save' -- --locked --test project_acceptance
```

## 実行結果

Linux の Rust 1.100.0 で受入テスト6件が通過した。cargo-mutants 27.1.0 で `Project::load` と `Project::save` を対象にした3件の変異は、2件を検出し、1件は `Project` が `Default` を実装していないためコンパイルできなかった。未検出とタイムアウトは0件だった。
