# pstplayer — Claude 向けプロジェクトルール

## commit 前の必須フォーマット (最重要)

`git commit` する前に**必ず** Rust と Frontend の両方を整形・検証すること。
本リポジトリは過去に rustfmt 違反を **4 連続コミット**で push し、CI の
Rust / Frontend ジョブを `fmt --check` で繰り返し落とした実績がある。

```bash
cargo fmt --all        # Rust 整形
npm run format         # Frontend 整形 (prettier --write)
# --- verify (CI と同じチェック) ---
cargo fmt --all -- --check
npm run lint           # prettier --check . && eslint .
```

- `.githooks/pre-commit` がこれを機械的に検証する。clone 後に一度
  `git config --local core.hooksPath .githooks` を実行して有効化する
  (CI 環境やフック未設定の環境でも、手動で上記を回すこと)。
- Web セッション・Local CLI セッションを問わず、Claude は例外なくこの手順を踏む。

## ビルド / 開発メモ

- Windows の MSVC ツールチェインは VS2019 (VS2022 は C++ 未導入)。`dev-env.ps1`
  を source して環境を整える。libmpv は `~/mpv-dev`、`mpv.lib` 必須。
- 開発時の実機確認で **vite (dev) は relaunch 時に古いコードを配信する**こと
  がある。リアクティビティ等を確実に検証したいときは vite を使わず
  `npx tauri build --debug --no-bundle` で本番フロント同梱の debug バイナリを
  作ってから `target/debug/pstplayer.exe` を起動する。

## アーキテクチャ補足

- 視聴画面 (`src/routes/+page.svelte`) は PeerCast 視聴 + BBS + libmpv 統合の
  単一画面。Svelte 5 runes。**注意**: 状態を多数持つ複雑なストアを別
  コンポーネントへ context 配布する構成では、`posts` 等の async 更新が一部
  コンポーネントにしか届かないリアクティビティ問題を踏むことがある。レス数
  などの表示は、`$derived` を経由せず**テンプレートに直接バインド**するのが
  最も確実 (例: `.status-bar` に `{posts.length}` を直書き)。
