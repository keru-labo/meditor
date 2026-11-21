# Rust の行カバレッジ

Rust のユニット・受入テストと、字幕10,000件を開く再生スモークテストを計測する。Linux の仮想画面と SDL のダミー音声デバイスを使い、合成素材を作成して Rust エディターを起動する。

| 集計範囲 | チェック |
| --- | --- |
| 再生制御の `rust/src/lib.rs` とプロジェクトモデルの `rust/src/project.rs` | 2ファイルを合計した行カバレッジが85%以上 |
| エディターの Rust コード全体 | 行カバレッジを報告 |

全体集計には GUI、映像デコード、音声ミックスの Rust 側の実装を含める。統合テストのファイル、合成素材生成ツール、依存ライブラリ、生成された UI コードは集計から外す。Rust の実装ファイルに併設されたユニットテストは LLVM の集計に含まれる。85%の基準は、先行してテストを整えた再生制御とプロジェクトモデルに適用する。Linux の計測は Windows 実機の日本語入力やデバイス連携の検証を補完する。

## GitHub Actions で確認

Rust のコードまたは計測設定を変更すると **Rust coverage** が実行される。手動実行もできる。ジョブのサマリーに範囲別の行カバレッジが表示され、`rust-coverage` アーティファクトから LCOV と LLVM の JSON サマリーを取得できる。

## ローカルで再現

[Linux での開発時の検証](../README.md#linux-での開発時の検証)に記載したライブラリと仮想画面に加えて `jq` を用意し、リポジトリのルートで実行する。

```sh
rustup toolchain install 1.100.0 --profile minimal --component llvm-tools-preview
cargo +1.100.0 install cargo-llvm-cov --version 0.9.1 --locked
RUSTUP_TOOLCHAIN=1.100.0 bash tools/coverage.sh
```

計測結果は `artifacts/coverage/` に出力する。テスト結果とスモークテストの実行履歴はその回の計測で集め直す。既存の [Windows と Linux の CI](../.github/workflows/native.yml) でも通常ビルドを継続して確認する。
