# PSTPlayer

クロスプラットフォームな PeerCast 視聴ソフトウェア。

[PCRPlayer](http://pecatv.s25.xrea.com/) (Windows 専用、開発停止中) を参考に、PeerCast の視聴・コンタクト URL に紐付く掲示板の閲覧と書き込みを 1 つのアプリで完結させることを目指す再実装プロジェクト。

## ステータス

設計完了、フェーズ 1 (スケルトン構築) 進行中。

## 開発

### 必要なツール

- Rust stable (1.78+) — `rustup install stable`
- Node.js 22 LTS
- OS 別の Tauri 依存:
  - **Linux (Debian/Ubuntu 24.04)**:
    ```
    sudo apt-get install -y libwebkit2gtk-4.1-dev librsvg2-dev \
      libsoup-3.0-dev libayatana-appindicator3-dev libxdo-dev pkg-config
    ```
  - **macOS**: Xcode CLT (`xcode-select --install`)
  - **Windows**: WebView2 Runtime (Windows 11 は同梱)、Visual Studio Build Tools

### セットアップ

```sh
npm install
```

### よく使うコマンド

| コマンド                                    | 内容                              |
| ------------------------------------------- | --------------------------------- |
| `npm run dev`                               | フロントだけ起動 (ブラウザ閲覧用) |
| `npm run check`                             | TypeScript / Svelte の型チェック  |
| `npm run lint`                              | Prettier + ESLint                 |
| `npm run format`                            | Prettier で自動整形               |
| `npm run build`                             | フロントエンドの静的出力          |
| `npm run tauri dev`                         | Tauri デスクトップアプリで起動    |
| `npm run tauri build`                       | 配布バイナリ生成                  |
| `cargo test` (`src-tauri/` 内)              | Rust のユニットテスト             |
| `cargo clippy --all-targets -- -D warnings` | Rust のリント                     |
| `cargo fmt --all`                           | Rust の整形                       |

### CI

`.github/workflows/ci.yml` で 3 OS (Ubuntu / macOS / Windows) のマトリクスビルド、Rust fmt / clippy / test、フロントの check / lint / build を実行します。

## スコープ

**PCRPlayer のオンライン視聴機能 (PeerCast 配信 + 連動掲示板) を、ライセンス問題を回避しながら同等に提供する** ことが目的。ローカル動画再生・シークバー・DirectShow フィルタグラフ等のオフライン/Windows 固有機能はオミット。詳細は [`docs/decisions/0004-scope.md`](docs/decisions/0004-scope.md)。

## 技術スタック

- **アプリ基盤**: [Tauri 2](https://tauri.app/) (Rust + Web フロントエンド)
- **バックエンド (Rust)**: PeerCast 通信、BBS スクレイピング、設定管理
- **フロントエンド**: Svelte 5 + SvelteKit + TypeScript + Vite
- **動画再生**: [libmpv](https://mpv.io/) (組み込み、未着手)
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
