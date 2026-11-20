# meditor

AI 動画編集ツールの新規プロジェクト。最初の開発対象は編集と動画プレビューの基盤です。

## 起動

Qt 6.5 以降（Quick、Quick Controls、Quick Dialogs、Multimedia）、CMake 3.21 以降、C++17 コンパイラを用意します。

```sh
cmake -S . -B build
cmake --build build --config Release
ctest --test-dir build -C Release --output-on-failure
```

単一構成では `build/meditor`、Visual Studio では `build/Release/meditor.exe` を起動します。実行ファイルに動画パスを渡すか、画面の **Open video** からローカル動画を開けます。Qt を標準外にインストールした場合は Qt SDK の `qt-cmake` で構成します。

Qt SDK なしでシークの C++ テストを構成するには `-DMEDITOR_BUILD_APP=OFF` を指定します。品質検査には Python 3、GCC/gcov、Lean が必要です。

```sh
python3 scripts/check_docs.py
python3 scripts/check_seek.py
lean proof/LatestSeek.lean
```

カバレッジ 85% のゲートは自前のシーク要求ポリシーに適用します。Lean は要求の上書き、消費、取消、範囲を状態モデルで検証します。Qt アダプターと描画は [GUI の受入手順](docs/acceptance.md) で確認します。

[再生基盤の設計と Windows 計測条件](docs/performance.md)
