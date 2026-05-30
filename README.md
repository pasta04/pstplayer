# PSTPlayer

クロスプラットフォームな PeerCast 視聴ソフトウェア。

[PCRPlayer](http://pecatv.s25.xrea.com/) (Windows 専用、開発停止中) を参考に、PeerCast の視聴・コンタクト URL に紐付く掲示板の閲覧と書き込みを 1 つのアプリで完結させることを目指す再実装プロジェクト。

## ステータス

設計段階。実装は未着手。

## 技術スタック

- **アプリ基盤**: [Tauri](https://tauri.app/) (Rust + Web フロントエンド)
- **バックエンド (Rust)**: PeerCast 通信、BBS スクレイピング、設定管理
- **フロントエンド**: TypeScript + Web UI フレームワーク (未確定)
- **動画再生**: [libmpv](https://mpv.io/) (組み込み)
- **対応プラットフォーム** (優先順): Windows → macOS → Linux

詳細は [`docs/architecture.md`](docs/architecture.md) を参照。

## ライセンス

MIT License。PCRPlayer (GPL v3) のコードは参照せず、公開プロトコル仕様 (PeerCast HTTP/PCP、2ch/したらば BBS API) からのクリーンルーム実装とする。詳細は [`docs/decisions/0002-license-clean-room.md`](docs/decisions/0002-license-clean-room.md)。

## ドキュメント

- [`docs/architecture.md`](docs/architecture.md) — システムアーキテクチャ
- [`docs/features.md`](docs/features.md) — 機能仕様
- [`docs/ui-design.md`](docs/ui-design.md) — UI レイアウト・インタラクション
- [`docs/roadmap.md`](docs/roadmap.md) — 開発ロードマップ (フェーズ計画)
- [`docs/protocols/peercast.md`](docs/protocols/peercast.md) — PeerCast プロトコル参考メモ
- [`docs/protocols/bbs.md`](docs/protocols/bbs.md) — BBS プロトコル参考メモ
- [`docs/decisions/`](docs/decisions/) — アーキテクチャ決定記録 (ADR)

## 参考プロジェクト

- [PCRPlayer](http://pecatv.s25.xrea.com/) — オリジナルの Windows 版 (GPL v3)
- [PeerCastStation](https://github.com/kumaryu/peercaststation) — 現行 PeerCast 本体 (GPL v3)、API 仕様の参考元
- [pcoplayer](https://github.com/progre/pcoplayer) — 同様のクロスプラットフォーム移植 (Tauri、MIT、開発停止)
