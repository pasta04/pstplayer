# PSTPlayer

クロスプラットフォームな PeerCast 視聴ソフトウェア。

[PCRPlayer](http://pecatv.s25.xrea.com/) (Windows 専用、開発停止中) を参考に、PeerCast の視聴・コンタクト URL に紐付く掲示板の閲覧と書き込みを 1 つのアプリで完結させることを目指す再実装プロジェクト。

## ステータス

設計完了、フェーズ 1 (MVP) 実装中。

| フェーズ          | 状況                                                                 |
| ----------------- | -------------------------------------------------------------------- |
| 1.1 スケルトン    | ✅ Tauri 2 + Svelte 5 + Vite + CI + lint 一式                        |
| 1.2 PeerCast 連携 | ✅ URL/playlist パーサ、JSON-RPC、legacy admin、戦略選択、接続先解決 |
| 1.3 動画再生      | ⚠ libmpv 統合まで完了。ウィンドウ埋め込み (`wid` プロパティ) のみ残  |
| 1.4 BBS 連携      | ✅ したらば / 2ch 互換、subject/dat、差分取得、投稿、HTML サニタイズ |
| 1.5 UI            | ✅ 2 ペインレイアウト、URL ペーストから視聴/書き込みまで疎通         |
| 1.6 設定          | ✅ TOML 永続化 + コマンド (UI からの編集画面は別途)                  |

Rust テスト 72 件 (うち 71 が `pst-core`、1 が libmpv 初期化テスト)、全 pass。

## 開発

### リポジトリ構造

```
pstplayer/
├── Cargo.toml                  ← workspace root
├── crates/
│   ├── pst-core/               ← UI 非依存ロジック (PeerCast / BBS / config)
│   └── pst-server/             ← (フェーズ 4 MVP) axum HTTP API。モバイル向け
├── src-tauri/                  ← Tauri デスクトップアプリ
│   └── src/{commands, player}  ← Tauri command + libmpv 統合
├── src/                        ← フロントエンド (Svelte 5)
│   ├── routes/+page.svelte
│   └── lib/{api.ts, format.ts}
└── docs/                       ← 設計ドキュメント (ADR / プロトコル / UI)
```

### 必要なツール

- **Rust stable** (1.78+) — `rustup install stable`
- **Node.js 22 LTS**
- **OS 別の Tauri + libmpv 依存**:
  - **Linux (Debian/Ubuntu 24.04)**:
    ```sh
    sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev \
      libsoup-3.0-dev libayatana-appindicator3-dev libxdo-dev \
      libmpv-dev pkg-config
    ```
  - **macOS**: Xcode CLT (`xcode-select --install`) + `brew install mpv`
  - **Windows**: WebView2 Runtime (Windows 11 は同梱)、Visual Studio Build Tools、`mpv-dev` (vcpkg / choco)

### セットアップ

```sh
npm install
```

### よく使うコマンド

| コマンド                                                | 内容                                         |
| ------------------------------------------------------- | -------------------------------------------- |
| `npm run dev`                                           | フロントだけ起動 (ブラウザ閲覧用)            |
| `npm run check`                                         | TypeScript / Svelte の型チェック             |
| `npm run lint`                                          | Prettier + ESLint                            |
| `npm run format`                                        | Prettier で自動整形                          |
| `npm run build`                                         | フロントエンドの静的出力                     |
| `npm run tauri dev`                                     | Tauri デスクトップアプリで起動 (libmpv 必須) |
| `npm run tauri build`                                   | 配布バイナリ生成                             |
| `cargo test --workspace`                                | Rust ユニットテスト (workspace 全体)         |
| `cargo clippy --workspace --all-targets -- -D warnings` | Rust リント                                  |
| `cargo fmt --all`                                       | Rust 整形                                    |

### CI

ワークフローは 2 つに分かれています:

- **`.github/workflows/ci.yml`** — テストとリント (軽量、PR ごとに毎回)
  - 3 OS (Ubuntu / macOS / Windows) で `cargo fmt / clippy / test`
  - フロントの `svelte-check / lint / build`

- **`.github/workflows/build.yml`** — Tauri 配布バイナリのビルド (重め、**main push と手動起動のみ**)
  - 3 OS マトリクスで `npm run tauri build`
  - 生成された **配布バイナリを Actions の Artifacts にアップロード** (14 日間保持)
  - Windows は libmpv-dev のリンクが不安定なため `continue-on-error: true`

無料枠節約のため両 workflow に `concurrency: cancel-in-progress` を入れていて、同じブランチへ連続 push すると古い実行は自動的にキャンセルされます。

#### 配布バイナリの取得

main への push 時、または手動起動時に GitHub Actions が走り、各 OS のインストーラが artifact として保存されます。

1. リポジトリの **Actions タブ** → 該当の `Build artifacts` ワークフロー実行を開く
2. 画面下の **Artifacts** から OS 別にダウンロード:
   - `pstplayer-linux` — `.deb` / `.AppImage` / `.rpm`
   - `pstplayer-macos` — `.dmg` / `.app`
   - `pstplayer-windows-portable` — `pstplayer.exe` + `libmpv-2.dll` + `README.txt` の ZIP (即実行可能、インストール不要)
3. 手元で実行
   - **Linux**: AppImage は `chmod +x ./PSTPlayer*.AppImage && ./PSTPlayer*.AppImage` または .deb を `sudo dpkg -i`
   - **macOS**: .dmg をマウントして .app を Applications に
   - **Windows**: ZIP を解凍してフォルダ内の `pstplayer.exe` をダブルクリック (libmpv-2.dll が同じディレクトリにある必要あり)

**PR ブランチで artifact が欲しい場合**: Actions タブ → `Build artifacts` → **Run workflow** → 対象ブランチを選択 → 実行 (手動起動)

## スコープ

**PCRPlayer のオンライン視聴機能 (PeerCast 配信 + 連動掲示板) を、ライセンス問題を回避しながら同等に提供する** ことが目的。ローカル動画再生・シークバー・DirectShow フィルタグラフ等のオフライン/Windows 固有機能はオミット。詳細は [`docs/decisions/0004-scope.md`](docs/decisions/0004-scope.md)。

## 技術スタック

- **アプリ基盤**: [Tauri 2](https://tauri.app/) (Rust + Web フロントエンド)
- **バックエンド (Rust)**: PeerCast 通信、BBS スクレイピング、設定管理
- **フロントエンド**: Svelte 5 + SvelteKit + TypeScript + Vite
- **動画再生**: [libmpv](https://mpv.io/) (`libmpv2` クレート、組み込み)
- **対応プラットフォーム** (優先順): Windows → macOS → Linux

詳細は [`docs/architecture.md`](docs/architecture.md) を参照。

## ライセンス

MIT License。PCRPlayer (GPL v3) のコードは参照せず、公開プロトコル仕様 (PeerCast HTTP/PCP、2ch/したらば BBS API) からのクリーンルーム実装とする。詳細は [`docs/decisions/0002-license-clean-room.md`](docs/decisions/0002-license-clean-room.md)。

## ドキュメント

- [`docs/architecture.md`](docs/architecture.md) — システムアーキテクチャ
- [`docs/features.md`](docs/features.md) — 機能仕様
- [`docs/ui-design.md`](docs/ui-design.md) — UI レイアウト・インタラクション
- [`docs/shortcuts.md`](docs/shortcuts.md) — キーボードショートカット一覧
- [`docs/roadmap.md`](docs/roadmap.md) — 開発ロードマップ (フェーズ計画)
- [`docs/release.md`](docs/release.md) — リリース / 配布方針 (バージョニング・タグ運用・アイコン仕様)
- [`docs/usage/`](docs/usage/) — **ユーザマニュアル** (インストール / 初回設定 / 基本操作 / ショートカット / トラブルシューティング)
- [`docs/protocols/peercast.md`](docs/protocols/peercast.md) — PeerCast プロトコル参考メモ
- [`docs/protocols/bbs.md`](docs/protocols/bbs.md) — BBS プロトコル参考メモ
- [`docs/decisions/`](docs/decisions/) — アーキテクチャ決定記録 (ADR)
  - [`0001-tech-stack.md`](docs/decisions/0001-tech-stack.md) — 技術スタック
  - [`0002-license-clean-room.md`](docs/decisions/0002-license-clean-room.md) — ライセンス + クリーンルーム
  - [`0003-ui-framework.md`](docs/decisions/0003-ui-framework.md) — UI フレームワーク (Svelte 5)
  - [`0004-scope.md`](docs/decisions/0004-scope.md) — 機能スコープ (PCRPlayer 互換 + オフライン系オミット)
  - [`0005-workspace-and-server.md`](docs/decisions/0005-workspace-and-server.md) — Cargo ワークスペース化 + 将来のリレーサーバ (モバイル対応)

## 参考プロジェクト

- [PCRPlayer](http://pecatv.s25.xrea.com/) — オリジナルの Windows 版 (GPL v3)
- [PeerCastStation](https://github.com/kumaryu/peercaststation) — 現行 PeerCast 本体 (GPL v3)、API 仕様の参考元
- [pcoplayer](https://github.com/progre/pcoplayer) — 同様のクロスプラットフォーム移植 (Tauri、MIT、開発停止)
