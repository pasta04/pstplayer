# 開発者向けガイド

PSTPlayer をソースからビルド・開発する人向けのドキュメントです。
**ビルド済みバイナリの使い方**は [README](../README.md) と
[`docs/usage/`](usage/) の各ページを参照してください。

## リポジトリ構造

```
pstplayer/
├── Cargo.toml                  ← Cargo workspace root
├── crates/
│   ├── pst-core/               ← UI 非依存ロジック (PeerCast / BBS / config / single-instance)
│   └── pst-server/             ← axum HTTP API (常駐サーバ・モバイル向け配信)
├── src-tauri/                  ← Tauri デスクトップアプリ本体
│   └── src/{commands, player}  ← Tauri command + libmpv 統合
├── src/                        ← フロントエンド (Svelte 5 / SvelteKit / TypeScript)
│   ├── routes/+page.svelte     ← 視聴画面 (PeerCast + BBS + libmpv の単一画面モノリス)
│   ├── routes/{hub,yp,settings,threads}/  ← ハブ / YP / 設定 / スレ一覧ウィンドウ
│   └── lib/{api.ts, format.ts}
└── docs/                       ← 設計・開発・ユーザ向けドキュメント
```

## 必要なツール

- **Rust stable** (1.78+) — `rustup install stable`
- **Node.js 24 LTS** (Active LTS 推奨。Node 22 は Maintenance フェーズ)
- **OS 別の Tauri + libmpv 依存**:
  - **Linux (Debian / Ubuntu 24.04)**:
    ```sh
    sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev \
      libsoup-3.0-dev libayatana-appindicator3-dev libxdo-dev \
      libmpv-dev pkg-config
    ```
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`) + `brew install mpv`
  - **Windows**: WebView2 Runtime (Windows 11 は同梱)、Visual Studio Build Tools (C++)、`mpv-dev` (vcpkg / choco)。MSVC ツールチェインは VS2019 で検証 (VS2022 は C++ 未導入の環境あり)。

## セットアップ

```sh
npm install
git config --local core.hooksPath .githooks   # commit 前フォーマット検証フックを有効化
```

## よく使うコマンド

| コマンド                                                | 内容                                         |
| ------------------------------------------------------- | -------------------------------------------- |
| `npm run dev`                                           | フロントだけ起動 (ブラウザ閲覧用)            |
| `npm run check`                                         | TypeScript / Svelte の型チェック             |
| `npm run lint`                                          | Prettier + ESLint (CI と同じ検証)            |
| `npm run format`                                        | Prettier で自動整形                          |
| `npm run build`                                         | フロントエンドの静的出力                     |
| `npm run tauri dev`                                     | Tauri デスクトップアプリで起動 (libmpv 必須) |
| `npm run tauri build`                                   | 配布バイナリ生成                             |
| `cargo test --workspace`                                | Rust ユニットテスト (workspace 全体)         |
| `cargo clippy --workspace --all-targets -- -D warnings` | Rust リント (CI と同じ検証)                  |
| `cargo fmt --all`                                       | Rust 整形                                    |

### vite dev の注意

実機での動作確認では、`vite` (dev) が relaunch 時に古いコードを配信する
ことがあります。リアクティビティ等を確実に検証したいときは vite を使わず、
`npx tauri build --debug --no-bundle` で本番フロント同梱の debug バイナリを
作ってから `target/debug/pstplayer` を起動してください。

## commit 前の必須フォーマット (重要)

`git commit` の前に **必ず** Rust と Frontend の両方を整形・検証してください。
本リポジトリは過去に rustfmt 違反を複数回連続で push し、CI を繰り返し
落とした実績があります。

```sh
cargo fmt --all        # Rust 整形
npm run format         # Frontend 整形 (prettier --write)
# --- verify (CI と同じチェック) ---
cargo fmt --all -- --check
npm run lint           # prettier --check . && eslint .
```

`git config --local core.hooksPath .githooks` を実行しておくと、
`.githooks/pre-commit` が上記を機械的に検証してコミットをブロックします。

## CI

GitHub Actions のワークフローは 2 つに分かれています:

- **`.github/workflows/ci.yml`** — テストとリント (軽量、PR ごとに毎回)
  - 3 OS (Ubuntu / macOS / Windows) で `cargo fmt / clippy / test`
  - フロントの `svelte-check / lint / build`
  - 完了時に PR へ結果サマリをコメントする `notify-pr` ジョブ付き
- **`.github/workflows/build.yml`** — Tauri 配布バイナリのビルド (重め、**main push と手動起動のみ**)
  - 3 OS マトリクスで `npm run tauri build`
  - 生成された配布バイナリを Actions の Artifacts にアップロード (14 日保持)
  - Windows は libmpv-dev のリンクが不安定なため `continue-on-error: true`

無料枠節約のため両 workflow に `concurrency: cancel-in-progress` を入れており、
同じブランチへ連続 push すると古い実行は自動的にキャンセルされます。

## 配布バイナリのビルド

### ローカルでビルド

```sh
npm run tauri build
```

成果物は `src-tauri/target/release/bundle/` 以下に OS 別形式で出力されます。

### CI の Artifacts から取得

`main` への push 時、または手動起動時に `Build artifacts` ワークフローが走り、
各 OS のインストーラが artifact として保存されます。

1. リポジトリの **Actions タブ** → 該当の `Build artifacts` 実行を開く
2. 画面下の **Artifacts** から OS 別にダウンロード
   - `pstplayer-linux` — `.deb` / `.AppImage` / `.rpm`
   - `pstplayer-macos` — `.dmg` / `.app`
   - `pstplayer-windows-portable` — `pstplayer.exe` + `libmpv-2.dll` + `README.txt` の ZIP
3. **PR ブランチで artifact が欲しい場合**: Actions タブ → `Build artifacts` → **Run workflow** → 対象ブランチを選択 → 実行

## 実装状況

| フェーズ                  | 状況                                                                            |
| ------------------------- | ------------------------------------------------------------------------------- |
| 1.1 スケルトン            | ✅ Tauri 2 + Svelte 5 + Vite + CI + lint 一式                                   |
| 1.2 PeerCast 連携         | ✅ URL/playlist パーサ、JSON-RPC、legacy admin、戦略選択、接続先解決            |
| 1.3 動画再生              | ⚠ libmpv 統合済。ウィンドウ埋め込み (`wid`) は実機 QA 反復中                    |
| 1.4 BBS 連携              | ✅ したらば / 2ch 互換、subject/dat、差分取得、投稿、HTML サニタイズ            |
| 1.5 UI                    | ✅ 2 ペインレイアウト、URL ペーストから視聴/書き込みまで疎通                    |
| 1.6 設定                  | ✅ TOML 永続化 + コマンド + 設定ダイアログ                                      |
| 2.1 ハブ画面              | ✅ PeCaRecorder 風テーブル UI、複数 YP、ソート、フィルタ、右クリックメニュー    |
| 2.2 別プロセス視聴        | ✅ ハブから行クリックで viewer を spawn、channel_id 単位の single-instance lock |
| 2.3 自動配信録画          | ✅ pst-server に AutoRecorder task (お気に入りルール `auto_record=true` で発火) |
| 2.4 Web グリッド (Server) | ✅ pst-server Web に `<video>` × N のグリッド表示                               |

フェーズ計画の詳細は [`roadmap.md`](roadmap.md) を参照。

## 設計ドキュメント

- [`architecture.md`](architecture.md) — システムアーキテクチャ
- [`features.md`](features.md) — 機能仕様
- [`ui-design.md`](ui-design.md) — UI レイアウト・インタラクション
- [`shortcuts.md`](shortcuts.md) — キーボードショートカット一覧
- [`usecases.md`](usecases.md) — ユースケース一覧
- [`roadmap.md`](roadmap.md) — 開発ロードマップ
- [`release.md`](release.md) — リリース / 配布方針
- [`qa-checklist.md`](qa-checklist.md) — リリース前の実機 QA チェックリスト
- [`design/`](design/) — デザインスケッチ (ハブ / Web グリッドのモックアップ)
- [`protocols/peercast.md`](protocols/peercast.md) — PeerCast プロトコル参考メモ
- [`protocols/bbs.md`](protocols/bbs.md) — BBS プロトコル参考メモ
- [`decisions/`](decisions/) — アーキテクチャ決定記録 (ADR)
  - [`0001-tech-stack.md`](decisions/0001-tech-stack.md) — 技術スタック
  - [`0002-license-clean-room.md`](decisions/0002-license-clean-room.md) — ライセンス + クリーンルーム
  - [`0003-ui-framework.md`](decisions/0003-ui-framework.md) — UI フレームワーク (Svelte 5)
  - [`0004-scope.md`](decisions/0004-scope.md) — 機能スコープ
  - [`0005-workspace-and-server.md`](decisions/0005-workspace-and-server.md) — Cargo ワークスペース化 + リレーサーバ
  - [`0006-auto-record-and-multiview.md`](decisions/0006-auto-record-and-multiview.md) — 自動配信録画 + 複数チャンネル視聴
