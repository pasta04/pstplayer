# PeerCast プロトコル参考メモ

PSTPlayer が PeerCast (主に [PeerCastStation](https://github.com/kumaryu/peercaststation)) と通信するために必要なプロトコルの公開仕様まとめ。

**スコープ**: クライアントとして PeerCast に接続し、チャンネル情報の取得・操作とストリーム視聴を行うために必要な範囲のみ。リレー側 (PCP の細部) は当面実装しない。

**参考元** (公開ドキュメント):

- [PeerCastStation GitHub](https://github.com/kumaryu/peercaststation)
- [PeerCastStation Wiki — JSON RPC API メモ](https://github.com/kumaryu/peercaststation/wiki/JSON-RPC-API-%E3%83%A1%E3%83%A2)
- [PeerCastStation Wiki — index.txt の仕様](https://github.com/kumaryu/peercaststation/wiki/index.txt%E3%81%AE%E4%BB%95%E6%A7%98)
- [PeerCastStation Wiki — PCP プロトコルメモ](https://github.com/kumaryu/peercaststation/wiki/PCP%E3%83%97%E3%83%AD%E3%83%88%E3%82%B3%E3%83%AB%E3%83%A1%E3%83%A2)
- [PeerCastStation Wiki — PCP パケットの構造](https://github.com/kumaryu/peercaststation/wiki/PCP%E3%83%91%E3%82%B1%E3%83%83%E3%83%88%E3%81%AE%E6%A7%8B%E9%80%A0)

---

## 1. 接続前提

- 既定リスニングポート: **7144** (TCP)
- 既定ホスト: `localhost` (リモート視聴対応は v1.0 以降)
- 認証: ローカル接続は不要。リモート時は HTTP Basic 認証

## 2. ストリーム視聴 URL

クライアントがストリームを取得するための URL 形式 (PeerCastStation を含むほとんどの実装で互換):

| URL                                              | 内容                                              |
| ------------------------------------------------ | ------------------------------------------------- |
| `http://host:port/pls/{id}[.{ext}][?tip={tip}]`  | プレイリスト (PLS 形式)。中身に実ストリーム URL  |
| `http://host:port/stream/{id}.{ext}`             | 実ストリーム本体                                  |
| `http://host:port/play.html?id={id}`             | HTML 再生ページ (Web UI)                          |

- `{id}` は 32 桁の hex (チャンネル ID)
- `{ext}` はコンテナ種別 (`flv` / `mkv` / `wmv` / `webm` / `ts` 等)
- `host` 部は **ホスト名 (`localhost` 等)** と **IP アドレス** の両形式が来る
- `?tip={ip:port}` パラメータ: **トラッカー IP** (任意)。YP やチャンネルリストから渡される。PeerCast 本体がリレー接続の優先候補として使用
- PCRPlayer は `pls/{id}` を主に扱っていた
- libmpv は直接 `stream/` も `pls/` も読める

### URL の正規表現サンプル

```
^https?://
  (?P<host>[^:/]+)
  :(?P<port>\d+)
  /(?P<kind>pls|stream)
  /(?P<id>[0-9A-Fa-f]{32})
  (?:\.(?P<ext>[a-z0-9]+))?
  (?:\?tip=(?P<tip>[^&]+))?
```

`kind` で `pls` / `stream` を判別し、`pls` の場合は HTTP GET → 中身パース → libmpv に渡す。

### 推奨クライアント挙動

1. ユーザが貼り付けた URL が `pls` か `stream` か `play.html` かを正規表現で識別
2. `pls` の場合は HTTP GET → 中身をパース → 1 行目の URL を libmpv に渡す
3. `stream` の場合はそのまま libmpv に渡す
4. `play.html?id={id}` の場合は `pls/{id}` に変換して扱う
5. チャンネル ID は別途保持 (停止・bump API で使う)

## 3. チャンネル情報・操作 API

### 3.1 新方式: JSON-RPC 2.0 (PeerCastStation 推奨)

```
POST http://host:port/api/1
Headers:
  X-Requested-With: XMLHttpRequest
  Content-Type: application/json
  Authorization: Basic xxx  ← リモート時のみ
Body:
  {"jsonrpc":"2.0","id":1,"method":"<methodName>","params":[...]}
```

主要メソッド (クライアントで使う想定):

| メソッド                         | 用途                                                       |
| -------------------------------- | ---------------------------------------------------------- |
| `getVersionInfo`                 | バージョン、API 互換性確認                                 |
| `getStatus`                      | 起動時間、ファイアウォール状態、エンドポイント            |
| `getChannels`                    | 全チャンネル + 状態 + メタ情報 + Track                     |
| `getChannelInfo(id)`             | 名前、URL、ジャンル、ビットレート、コンテナ種別、Track    |
| `getChannelStatus(id)`           | 状態、uptime、リレー数、direct 数、放送中フラグ           |
| `getChannelConnections(id)`      | source/relay/direct コネクション一覧                      |
| `bumpChannel(id)`                | 再接続                                                     |
| `stopChannel(id)`                | 切断                                                       |

### 3.2 旧方式: HTML 管理ページ (フォールバック用)

PCRPlayer が使っていた旧 API。古い PeerCast 本体や他実装で動かない場合の予備。

| URL                                              | 用途                |
| ------------------------------------------------ | ------------------- |
| `http://host:port/admin?cmd=viewxml`             | 全チャンネル情報 XML |
| `http://host:port/admin?cmd=viewxml&id={id}`     | 特定チャンネル XML   |
| `http://host:port/admin?cmd=stop&id={id}`        | 停止                |
| `http://host:port/admin?cmd=bump&id={id}`        | 再接続              |

XML の主要要素 (root `<peercast>` 配下):

- `<channels_found total="...">`
  - `<channel>` (属性): `name`, `id`, `bitrate`, `type`, `genre`, `desc`, `url`, `uptime`, `comment`, `skips`, `age`, `bcflags`
    - `<relay>` (属性): `listeners`, `relays`, `hosts`, `status`, `firewalled`
    - `<track>` (属性): `title`, `artist`, `album`, `genre`, `contact`

### 3.3 API 選択戦略

```
1. /api/1 で getVersionInfo を試行
   ├─ 成功 → JSON-RPC を継続使用
   └─ 失敗 → /admin?cmd=viewxml を試行
              ├─ 成功 → 旧方式継続
              └─ 失敗 → エラーをフロントに通知
```

## 4. YP (Yellow Page) チャンネル一覧

各 YP が公開する `index.txt` を取得してパース。

### 形式

- 1 行 1 チャンネル
- 区切り: `<>`
- フィールド内の `<` `>` は `&lt;` `&gt;` でエスケープ
- 文字コード: UTF-8 (主要 YP)
- フィールド数: 19

### フィールド配置

| Index | 内容                              |
| ----- | --------------------------------- |
| 0     | チャンネル名                      |
| 1     | チャンネル ID (32 桁 hex)         |
| 2     | TIP (トラッカー IP:Port)          |
| 3     | コンタクト URL ← BBS との連携キー |
| 4     | ジャンル                          |
| 5     | 詳細                              |
| 6     | リスナー数                        |
| 7     | リレー数                          |
| 8     | ビットレート (kbps)               |
| 9     | タイプ (FLV/WMV/MKV 等)           |
| 10    | Track アーティスト                |
| 11    | Track アルバム                    |
| 12    | Track タイトル                    |
| 13    | Track コンタクト URL              |
| 14    | URL エンコードされたチャンネル名  |
| 15    | 配信時間 (`H:MM` 形式)            |
| 16    | 固定文字列 `click`                |
| 17    | コメント                          |
| 18    | `0` / `1` (フラグ)                |

### 特殊ケース

- 制限チャンネル: ID が全て `0`、TIP は `127.0.0.1`

## 5. PCP プロトコル (参考、当面実装しない)

リレー間通信に使う独自バイナリプロトコル。PSTPlayer は視聴クライアントなので不要だが、将来 P2P 直接接続を実装する場合の参考メモ。

### ハンドシェイク (HTTP 風)

```
GET /channel/{channelId} HTTP/1.0
x-peercast-pcp: 1
x-peercast-pos: 0
Connection: close
```

| レスポンス | 意味                                          |
| ---------- | --------------------------------------------- |
| 200        | 接続確立、Atom ストリーム開始                 |
| 404        | チャンネル不在                                |
| 503        | リダイレクト (PCP_HOST atom が ≤8 個流れる) |

### Atom 構造

```
[4 bytes name][4 bytes length][payload...]
```

- 数値はリトルエンディアン
- name は 4 文字、不足は NUL 埋め
- length の MSB が 1 のとき: payload は子 Atom (再帰)、MSB が 0 のとき: 生バイト列

### 主要 Atom

| 名前      | 役割                              |
| --------- | --------------------------------- |
| `pcp\n`   | ヘッダ (バージョン 1)             |
| `helo`    | ハンドシェイク (クライアント情報) |
| `oleh`    | helo への応答                     |
| `chan`    | ストリーム本体 (HEAD/DATA/META)   |
| `host`    | 隣接ノード情報                    |
| `bcst`    | ブロードキャストラッパ            |
| `quit`    | 切断通知                          |

## 6. PSTPlayer 実装で必要なモジュール

```
peercast/
├── playlist.rs    # pls/m3u パーサ
├── jsonrpc.rs     # /api/1 クライアント
├── legacy_admin.rs# /admin?cmd= クライアント + XML パーサ
├── yp.rs          # index.txt パーサ
├── types.rs       # ChannelInfo, Status, Track, Relay 等の構造体
└── client.rs      # 上記を束ねる高レベル API (戦略選択も)
```

## 7. 既知の落とし穴 / 注意点

- **チャンネル ID の正規化**: 大文字小文字混在の場合がある。比較時は lower-case で揃える
- **JSON-RPC レスポンスのフィールドが nullable**: 未配信中のチャンネルは Track 情報が欠落しがち
- **`pls` ファイルが空のことがある**: チャンネル開始直後はストリーム URL が確定していない → リトライ必須
- **YP の `index.txt` が古い場合がある**: キャッシュされるので、表示時刻と TTL も保持する
- **ポート番号は固定ではない**: ユーザが PeerCast 本体の設定で変えていることがある → 設定で指定可
- **bcflags ビット**: トラックバー、フラグの組み合わせの解釈は YP 依存。詳細は実装時に調査
