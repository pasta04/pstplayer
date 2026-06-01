# PSTPlayer ユーザマニュアル

PSTPlayer は PeerCast 配信を視聴しながら関連 BBS (したらば / 2ch 互換)
を読み書きできるクロスプラットフォームのプレイヤーです。

## 目次

1. [インストール](install.md) — Windows / macOS / Linux でのダウンロードと初回起動
2. [初回セットアップ](first-setup.md) — PeerCast 接続先 / YP URL / 名前 等の設定
3. [基本操作](basic.md) — URL 貼り付け / YP 経由視聴 / BBS 読み書き
4. [ハブ画面](hub.md) — 複数 YP テーブル + お気に入り + 視聴管理 (PeCaRecorder 風)
5. [ショートカット一覧](shortcuts.md) — キーボード操作
6. [トラブルシューティング](troubleshooting.md) — よくある問題と対処
7. [pst-server](server.md) — Raspberry Pi 等の常駐サーバを LAN に置いて、
   モバイル端末のブラウザから視聴 + BBS 書き込みする

## 動作要件

- **PeerCast 本体** (本アプリ自身は PeerCast を内蔵しません):
  - 推奨: [PeerCastStation](https://github.com/kumaryu/peercaststation)
  - 同一マシンでも LAN 内の別マシンでも可
- **OS**:
  - Windows 10 以降 (x64)
  - macOS 11 (Big Sur) 以降
  - Linux (Ubuntu 22.04 / Fedora 39 等の最近のディストリ。`libmpv` 等
    のパッケージが必要)

## 機能の範囲 (含むもの / 含まないもの)

PSTPlayer は PCRPlayer (Windows 専用、開発停止) の代替を意識した実装
ですが、含まないものがあります:

含む:
- PeerCast 配信視聴 (libmpv 経由)
- したらば JBBS / 2ch 互換 BBS (5ch / jpnkn 等) の読み書き
- 設定の永続化、視聴履歴、スナップショット保存
- ホットキー (カスタマイズ可)
- YP チャンネル一覧 + 複数 YP 対応
- お気に入りルール (背景色 / 文字色 / 上位固定 / 自動録画)
- 録画 (libmpv `stream-record` 経由、再エンコード無し)
- **ハブ画面** (`pstplayer` を引数なしで起動した時の PeCaRecorder 風
  チャンネルテーブル + 複数視聴ウィンドウ管理)
- **常駐サーバ `pst-server`** (Pi / NAS に置いて自動配信録画 + モバイル
  ブラウザから視聴。詳細は [server.md](server.md))

含まない (将来検討):
- ローカル動画ファイルの再生
- シークバー
- DirectShow フィルタグラフ等の Windows 固有機能

詳細は [`docs/decisions/0004-scope.md`](../decisions/0004-scope.md) を
参照してください。

## 関連

- [GitHub Issues](https://github.com/pasta04/pstplayer/issues) — バグ報告 / 要望
- [`docs/roadmap.md`](../roadmap.md) — 開発計画
- [`docs/release.md`](../release.md) — リリース方針 (配布形式 / バージョニング)
