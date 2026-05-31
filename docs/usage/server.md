# pst-server (リレーサーバ) の使い方

`pst-server` は PSTPlayer のリレーサーバです。Raspberry Pi 等の常駐
サーバを LAN に置き、iPad / iPhone / Android / 他 PC のブラウザから
PeerCast 配信を視聴 + コンタクト掲示板を読み書きできるようにする
ことを目的とします。

## 想定環境

- Raspberry Pi 4 / 5 (Debian / Raspberry Pi OS)
- 紐付け先の **PeerCastStation** (LAN 内に 1 つ)
- 視聴端末: 同じ LAN 内のスマホ / タブレット / PC のブラウザ

## ビルドと配置

```bash
cargo build -p pst-server --release
# 出来上がる artifact:
#   target/release/pst-server
#   crates/pst-server/web/        ← 静的フロント (PWA)
```

配置例 (Raspberry Pi):

```bash
# 1. バイナリと web/ をコピー
sudo install -m 755 target/release/pst-server /usr/local/bin/
sudo mkdir -p /opt/pst-server
sudo cp -r crates/pst-server/web /opt/pst-server/web
# 2. 設定ファイル
sudo mkdir -p /etc/pst-server
sudo cp docs/usage/pst-server.example.toml /etc/pst-server/pst-server.toml
sudo $EDITOR /etc/pst-server/pst-server.toml
# 3. 起動
PST_SERVER_WEB_DIR=/opt/pst-server/web \
  pst-server --config /etc/pst-server/pst-server.toml
```

## 設定ファイル

`pst-server --config <path>` で明示指定可能。指定がない場合は OS 標準の
config_dir/PSTPlayer/pst-server.toml を参照します (デスクトップ版とは
別ファイル名なので 1 マシンで両方動かしても干渉しません)。

```toml
# 紐付け先 PeerCastStation
[peercast]
host = "192.0.2.10"
port = 7144
auth_user = ""
auth_pass = ""

[server]
bind = "0.0.0.0:8080"
public_url = "https://pst.example.lan/"   # 逆プロキシ越しの URL (manifest 用、任意)

# ログ出力 (既定 OFF)。
# Raspberry Pi の SD カード寿命対策として、本サーバは既定で一切ログを
# 書き出しません。デバッグが必要な時のみ有効化してください。
# 出力先は tmpfs (例: /run/pst-server/log) を推奨します。
[log]
debug = false
dir = ""
```

## ログ仕様

- **既定 (`log.debug = false`)**: 何も出力しない (`stderr` への致命的
  メッセージのみ、起動失敗時等)
- **デバッグ ON (`log.debug = true`, `log.dir = "/run/pst-server/log"` 等)**:
  指定ディレクトリに **日次ローテーション** で `pst-server.log.YYYY-MM-DD`
  を作成。`tracing-appender` の `non_blocking` writer 経由で、バッファ
  flush は exit 時のみ。 SD カードへの細かい書き込みを避ける設計。

> tmpfs (`/run/` 配下) は再起動でクリアされるため、原因調査で 1 回
> 動かして再起動という運用に最適。永続化したい場合は SSD や USB
> メモリ上を指定。

## HTTP API

主に Web フロントから叩く想定。手動でも使えます。

| パス                                | 動作                                                     |
| ----------------------------------- | -------------------------------------------------------- |
| `GET  /`                            | 静的フロント (PWA) を返す                                |
| `GET  /api/channels`                | 紐付け先の `getChannels` プロキシ                        |
| `GET  /api/channel/:id/info`        | チャンネル詳細                                            |
| `GET  /api/channel/:id/status`      | チャンネル状態 (リレー数 / 直接接続数 / 稼働時間 等)     |
| `POST /api/channel/:id/bump`        | 再接続 (Bump)                                            |
| `POST /api/channel/:id/stop`        | チャンネル切断                                            |
| `GET  /api/yp?url=...`              | 任意の YP `index.txt` をパースして返す                    |
| `GET  /api/board?url=...`           | BBS スレ一覧                                              |
| `GET  /api/thread?url=...&...`      | スレ取得 (`last_count`/`last_byte`/`last_modified`で差分) |
| `POST /api/thread/post` (JSON)      | BBS 書き込み                                              |
| `GET  /hls/:id`                     | PeerCastStation の HLS playlist をプロキシ                |
| `GET  /hls/:id/:segment`            | TS セグメントをプロキシ                                   |

エラー応答は `application/json` で `{ "code": "...", "message": "..." }`
の形式 (デスクトップ版と統一)。

## HLS と「ディスク不使用」方針

`pst-server` の HLS プロキシは **上流レスポンスを stream で透過** する
だけで、一時ファイル / FFmpeg / バッファ書き出しを行いません。
`reqwest::bytes_stream()` で受けたチャンクをそのまま axum の
`Body::from_stream` に渡すので、メモリ上を経由するだけです。

> 上流 PeerCastStation 側で HLS 出力を有効にする必要があります
> (配信プレイヤー設定で HLS を選択)。HLS 非対応のチャンネルは現状
> 視聴できません。FFmpeg リアルタイム変換は将来検討。

## ブラウザ対応

| ブラウザ                                                      | 再生方式                   |
| ------------------------------------------------------------- | -------------------------- |
| iPad / iPhone Safari                                          | `<video>` のネイティブ HLS |
| macOS Safari                                                  | 同上                       |
| **Windows Edge / Windows Chrome**                             | 同梱の **hls.js** 経由     |
| **Android Chrome**                                            | 同上                       |
| デスクトップ Chrome (Linux/macOS) / Firefox / Chromium 系全般 | 同上                       |

ネイティブ HLS を持っているのは Safari (iOS/macOS) のみで、Chromium /
Firefox 系は MSE (Media Source Extensions) ベースの `hls.js` で再生
します。`canPlayType('application/vnd.apple.mpegurl')` が空 (= 非対応)
の場合は自動的に hls.js 経路に切り替わるため、ユーザー側の設定や
プラグインインストールは不要です。

`hls.js` (Apache-2.0) を `crates/pst-server/web/vendor/hls.min.js` に
バンドル。ライセンス本文は同ディレクトリの `LICENSE-hls.js.txt`。差し
替える時は Service Worker のキャッシュバージョン (`sw.js` の `CACHE`
変数) も併せて bump してください。

## トラブルシューティング

### `/api/channels` が `502 peercast_unreachable` を返す

紐付け先 PeerCastStation (`peercast.host:port`) に届いていません。

- 設定ファイルのホスト / ポートを確認
- PeerCastStation の Web UI (`http://host:7144/`) がブラウザで開けるか
- ファイアウォール / iptables で 7144 が通っているか
- Basic 認証必須なら `auth_user` / `auth_pass` を設定

### 画面 (`/`) は出るがチャンネル一覧が出ない

ブラウザの開発ツール (Network) で `/api/channels` の response を確認。
上記の `peercast_unreachable` か、CORS の制限に引っかかっていないか。

### Pi の SD カードを長持ちさせたい

- `log.debug = false` のまま運用 (既定)
- 必要時のみ `log.dir = "/run/pst-server/log"` 等の tmpfs を指定
- HLS のキャッシュは `/tmp` 等にも書きません (stream 透過のため)

### Service Worker の更新

`crates/pst-server/web/sw.js` の `CACHE` バージョン名 (`pstplayer-web-v1`)
を変更すれば、ブラウザ再読み込み時に古いキャッシュが破棄されます。
