# 文書の検査

PR の **Check documentation and Gherkin** で、Markdown と Gherkin を検査する。文書が参照するファイルの移動・削除も検出するため、変更箇所にかかわらず実行する。リポジトリのルートで同じ検査を実行できる。Python と検査ライブラリは文書検査で使用する。

```sh
python -m pip install --require-hashes --only-binary=:all: -r tools/docs-check-requirements.txt
python tools/check-docs.py
```

- ルートの README と `docs/` の Markdown から、リポジトリ内のリンク先と Markdown の見出しアンカーを確認する。外部 URL は検査の対象外。
- `docs/features/` の Gherkin を構文解析し、シナリオアウトラインの例を展開する。空のシナリオや、展開されずに残った置換変数を検出する。
- 各 `.feature` と対応する機能定義を、[機能一覧](features/README.md) から参照できることを確認する。

Gherkin のステップを使う受入テストは、各機能の実装と合わせて検証する。仕様の意味、条件の抜け、利用者に見える結果はレビューで確認する。 [プレビュー版の受入条件](acceptance.md)
