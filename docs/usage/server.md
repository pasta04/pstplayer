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

# 録画機能。既定 OFF。enabled = true かつ dir が指定された時のみ
# /api/record/start が動く。書き出しは指定ディレクトリへ直接行うので、
# SD カードに書きたくない場合は外付け USB / SSD のマウント先を指定。
[recording]
enabled = false
dir = ""                       # 例: "/mnt/usb/recordings"
ext = ""                       # 空なら flv
max_concurrent = 0             # 0 = 既定 (8 本)。同時録画の上限
auto_poll_interval_sec = 0     # 0 = 既定 (60 秒)。自動録画 polling 間隔
auto_stop_grace_sec = 0        # 0 = 既定 (30 秒)。配信消滅後の停止猶予

# お気に入りルール。複数定義可、上から評価。フィールドは部分一致
# (大文字小文字無視)、空欄ワイルドカード、複数記述で AND。
# Web フロントは /api/favorites で読み出し、上位固定 / 背景色 /
# 自動録画に反映する。
[[favorites.rules]]
name = "メイン"
channel_name = "MOXch"
pin_top = true
auto_record = false
color = "#ff8a3d22"
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
| `GET  /api/record/list`             | 進行中の全録画タスク一覧                                   |
| `POST /api/record/start` (JSON)     | 録画開始 (`{id, name}`)。既定 OFF                          |
| `POST /api/record/stop` (JSON)      | 録画停止 (`{id}` で指定、省略時は全停止)                   |
| `GET  /api/favorites`               | 設定 `[[favorites.rules]]` を JSON で返す (read-only)      |
| `GET  /api/config`                  | 設定 (`pst-server.toml` 全体) を JSON で返す               |
| `PUT  /api/config` (JSON)           | 設定を書き戻し (ディスクに保存 + メモリ反映)               |
| `GET  /api/config/path`             | 現在使っている `pst-server.toml` のパス                    |

エラー応答は `application/json` で `{ "code": "...", "message": "..." }`
の形式 (デスクトップ版と統一)。

## 録画

`POST /api/record/start` で開始、`POST /api/record/stop` で停止。
同時録画は 1 本まで (バッティングしたら 409 `recording_busy`)。

- 既定 OFF (`[recording] enabled = false`)。SD カード書き込みを避け
  たい構成では有効化しないこと
- 有効化する場合は `dir` を **外付け USB / SSD のマウント先** に
  指定するのを推奨
- ファイル名は `YYYYMMDD_HHmmss_<channel_name>.<ext>` (Desktop 版と
  同じ規則)
- 上流 `http://{peercast}/stream/{id}.{ext}` を `bytes_stream` で
  受けて直接書き出します。再エンコード / 一時バッファ無し

Web フロント (`/`) では視聴中に右上の **⏺ 録画** ボタンで開始 /
停止できます。サーバ側で機能が無効な場合 (`enabled = false`) は
ボタン自体が隠れます。

### 自動配信録画

`favorites.rules` のうち `auto_record = true` のものは「該当する
配信が現れた瞬間に自動で録画開始」になります。

- `pst-server` 起動と同時にバックグラウンドで polling task が
  走り、`auto_poll_interval_sec` (既定 60 秒) 間隔で `getChannels`
  を取得します
- 一致した配信が既に録画中なら何もしません (毎周期 409 を吐かない)
- 一致した配信が `getChannels` から消えると `auto_stop_grace_sec`
  (既定 30 秒) 後に停止します。短時間の瞬断ではファイルが分割
  されません
- `[recording] enabled = false` または `auto_record = true` の
  ルールが 1 つも無いときは polling 自体走りません (待機ループのみ)

## Web UI からの設定変更

ヘッダ右上の **⚙** ボタン (または `/settings.html` を直接開く) で
`pst-server.toml` を編集できます。PeerCastStation Linux 版の Web
UI と似た形で、PeerCast 接続先 / サーバ / ログ / 録画 / お気に入り
ルールの全項目を編集して **💾 保存** でディスクと実行中プロセスの
両方に反映されます。

> ⚠ 現状認証が無いため、LAN 内に公開する時のみ使う想定です。WAN
> 公開時の認証 / 公開モードは別途実装予定 (ロードマップ参照)。

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
