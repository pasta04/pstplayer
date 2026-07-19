# PSTPlayer

PeerCast 配信を視聴しながら、コンタクト URL に紐付く掲示板 (したらば JBBS /
2ch 互換) を読み書きできるクロスプラットフォームのプレイヤーです。Windows
専用で開発停止した [PCRPlayer](http://pecatv.s25.xrea.com/) のオンライン視聴
機能を、Windows / macOS / Linux で使えるよう再実装しています。

> 本アプリ自身は PeerCast を内蔵しません。別途 PeerCast 本体
> (推奨: [PeerCastStation](https://github.com/kumaryu/peercaststation)) を
> 起動しておく必要があります (同一マシンでも LAN 内の別マシンでも可)。

## インストール

[GitHub Releases](https://github.com/pasta04/pstplayer/releases) からお使いの
OS 向けのファイルをダウンロードしてください。

- **Windows** — portable ZIP (解凍して `pstplayer.exe` を実行。`libmpv-2.dll` 同梱)
- **macOS** — `.app` (libmpv 同梱、Homebrew 不要)
- **Linux** — `.deb` / `.rpm` / `.AppImage` (システムの `libmpv` が必要)

OS 別の詳細手順 (初回起動時の警告回避など) は
[インストールガイド](docs/usage/install.md) を参照してください。

> v0.1.0 リリース前は、GitHub Actions の Artifacts (各ビルドごとに 14 日間
> 保持) からも取得できます。取得手順は [開発者向けガイド](docs/development.md)
> を参照。

## 使い方 (ユーザマニュアル)

1. [インストール](docs/usage/install.md) — ダウンロードと初回起動
2. [初回セットアップ](docs/usage/first-setup.md) — PeerCast 接続先 / YP URL / 名前 等の設定
3. [基本操作](docs/usage/basic.md) — URL 貼り付け / YP 経由視聴 / BBS 読み書き
4. [ハブ画面](docs/usage/hub.md) — 複数 YP テーブル + お気に入り + 視聴管理 (PeCaRecorder 風)
5. [ショートカット一覧](docs/usage/shortcuts.md) — キーボード操作
6. [トラブルシューティング](docs/usage/troubleshooting.md) — よくある問題と対処
7. [pst-server](docs/usage/server.md) — 常駐サーバを LAN に置き、モバイル端末の
   ブラウザから視聴 + BBS 書き込みする

## 動作要件

- **PeerCast 本体** (内蔵しません):
  - 推奨: [PeerCastStation](https://github.com/kumaryu/peercaststation)
  - 同一マシンでも LAN 内の別マシンでも可
- **OS**:
  - Windows 10 以降 (x64)
  - macOS 11 (Big Sur) 以降
  - Linux (Ubuntu 22.04 / Fedora 39 等の最近のディストリ。`libmpv` パッケージが必要)

## 機能の範囲

含むもの:

- PeerCast 配信視聴 (libmpv 経由)
- したらば JBBS / 2ch 互換 BBS (5ch / jpnkn 等) の読み書き
- 設定の永続化、視聴履歴、スナップショット保存
- ホットキー (カスタマイズ可)
- YP チャンネル一覧 + 複数 YP 対応
- お気に入りルール (背景色 / 文字色 / 上位固定 / 自動録画)
- 録画 (libmpv `stream-record` 経由、再エンコード無し)
- ハブ画面 (引数なし起動時の PeCaRecorder 風チャンネルテーブル + 複数視聴ウィンドウ管理)
- 常駐サーバ `pst-server` (Pi / NAS に置いて自動配信録画 + モバイルブラウザから視聴)

含まないもの (将来検討):

- ローカル動画ファイルの再生 / シークバー
- DirectShow フィルタグラフ等の Windows 固有機能

詳細は [`docs/decisions/0004-scope.md`](docs/decisions/0004-scope.md) を参照。

## ライセンス

MIT License。PCRPlayer (GPL v3) のコードは参照せず、公開プロトコル仕様
(PeerCast HTTP/PCP、2ch/したらば BBS API) からのクリーンルーム実装です。
詳細は [`docs/decisions/0002-license-clean-room.md`](docs/decisions/0002-license-clean-room.md)。

## 開発者向け

ソースからのビルド・開発手順 (必要ツール / `npm`・`cargo` コマンド / CI /
配布バイナリのビルド) は **[開発者向けガイド `docs/development.md`](docs/development.md)**
にまとめています。アーキテクチャ / ADR / プロトコル等の設計ドキュメントへの
リンクも同ファイルにあります。

## 参考プロジェクト

- [PCRPlayer](http://pecatv.s25.xrea.com/) — オリジナルの Windows 版 (GPL v3)
- [PeerCastStation](https://github.com/kumaryu/peercaststation) — 現行 PeerCast 本体 (GPL v3)、API 仕様の参考元
- [pcoplayer](https://github.com/progre/pcoplayer) — 同様のクロスプラットフォーム移植 (Tauri、MIT、開発停止)

## 関連

- [GitHub Issues](https://github.com/pasta04/pstplayer/issues) — バグ報告 / 要望
- [`docs/roadmap.md`](docs/roadmap.md) — 開発計画
- [`docs/release.md`](docs/release.md) — リリース方針 (配布形式 / バージョニング)
