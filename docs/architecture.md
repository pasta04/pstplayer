# アーキテクチャ

PSTPlayer の全体構成、モジュール分割、外部依存関係をまとめる。

## 全体像

PSTPlayer は **デスクトップアプリ (`pstplayer`)** と **常駐サーバ
(`pst-server`)** の 2 バイナリ + 共通ロジック (`pst-core`) からなる
Cargo workspace。前者はユーザが日常使う Tauri アプリ、後者は Raspberry
Pi 等の常駐機 (もしくは同じ PC 上のバックグラウンド) で自動配信録画 +
モバイル / 他 PC からの Web 視聴を担う。両者は同じ PeerCast プロトコル
クライアントと BBS クライアントを共有する。

```
                       ┌─────────────────────────────────────────┐
                       │  pst-core (library crate)               │
                       │  - peercast/ (JSON-RPC, legacy admin,   │
                       │    URL/playlist パーサ, YP index.txt)   │
                       │  - bbs/ (したらば / 2ch 互換 + 文字     │
                       │    コード変換 + HTML サニタイズ)        │
                       │  - config/ (TOML schema + load/save)    │
                       │  - favorites (ルール評価)               │
                       │  - snapshot (録画/スナップショット      │
                       │    出力先 + ファイル名生成)             │
                       │  - single_instance (ロックファイル +    │
                       │    TCP IPC、Desktop 多重起動制御)       │
                       │  - cli (CLI 引数パース)                 │
                       └────────┬───────────────────┬────────────┘
                                │                   │
                  ┌─────────────┴────┐    ┌─────────┴──────────┐
                  │  pstplayer       │    │  pst-server         │
                  │  (Tauri 2 desktop)│    │  (axum HTTP + PWA) │
                  └──────────────────┘    └─────────────────────┘
```

### pstplayer (Desktop)

引数なし起動 = **ハブ画面**、URL 引数あり = **ビューア画面** の 2 モード
を 1 バイナリで提供する。

```
pstplayer (引数なし)              pstplayer <url>                     pstplayer <url> <name> --record-on-start
        ↓                                ↓                                       ↓
┌────────────────────┐          ┌────────────────────┐              ┌──────────────────────────┐
│ ハブ画面 (/hub)    │          │ ビューア画面 (/)   │              │ ビューア画面 (録画 ON 起動) │
│ - 複数 YP テーブル │          │ - libmpv 再生      │              │                          │
│ - お気に入り       │          │ - BBS ペイン       │              │                          │
│ - 視聴中 / 録画中  │ ─spawn─▶ │ - スナップショット │              │                          │
│   タブ + 一括閉じ  │          │ - 録画 (任意)      │              │                          │
└────────────────────┘          └────────────────────┘              └──────────────────────────┘
        ▲                                ▲
        │  IPC (TCP 127.0.0.1, OK 応答付き)│
        │  - focus / close                │
        │  - state (録画中?)              │
        │  - startrec / stoprec           │
        └────────────────────────────────┘
```

ビューアは **channel_id 単位の single_instance ロック** で多重起動を
排除する。`acquire(channel_id)` 時に `$TMPDIR/pstplayer-locks/ch-{id}.lock`
を `create_new(true)` で原子的に作り、`127.0.0.1:0` で TCP listener を
bind して `pid` と `ipc_addr` を書き込む。後続プロセスがロックを見つけ
たら `ping/pong` で生死確認し、生きていれば `focus` 要求を送って自分は
終了する (= ユーザは同じチャンネルを 2 度起動しても 1 ウィンドウに
集約される)。

### pst-server (常駐 + モバイル / 他 PC ビューア)

```
┌──────────────────────────┐
│  pst-server (axum)       │
│                          │
│  /api/channels           │← 上流 PeerCastStation の getChannels プロキシ
│  /api/channel/{id}/...   │  (info / status / bump / stop)
│  /api/yp                 │  YP プロキシ (CORS 回避)
│  /api/board?url=         │  BBS subject.txt プロキシ
│  /api/thread?url=        │  BBS dat / rawmode プロキシ
│  /api/thread/post        │  BBS 書き込みプロキシ
│  /api/record/...         │  start / stop / list (HashMap<id, JoinHandle>)
│  /api/favorites          │  read-only
│  /api/config             │  read / write (LAN 想定、認証なし)
│  /hls/{id}.m3u8          │  HLS playlist プロキシ
│  /hls/{id}/{seg}         │  HLS segment プロキシ (Range 透過)
│  /                       │  静的 Vanilla JS PWA (Service Worker + manifest)
│                          │
│  AutoRecorder task ───── │  起動と同時に getChannels を
│   (tokio::spawn)         │  auto_poll_interval_sec 間隔で polling、
│                          │  auto_record=true ルールに一致した
│                          │  チャンネルを RecordingState.start
└──────────────────────────┘
```

録画は HashMap<channel_id, RecordingTask> + tokio::spawn でチャンネル毎
に独立タスク。`max_concurrent` まで並行可。graceful 停止は
`stop_flag (AtomicBool)` を立てて次チャンク境界で抜ける → 1 秒猶予 →
fallback abort。

### バイナリの組み合わせ

| 用途 | 動かすもの |
| ---- | ---------- |
| Desktop 単体視聴 (ペースト + YP) | `pstplayer` のみ |
| 自動録画 + 常駐 | `pst-server` (Pi / NAS / 同 PC バックグラウンド) |
| モバイル / タブレットから視聴 | `pst-server` + ブラウザ |
| グリッド (N 配信同時視聴) | `pst-server` Web UI のグリッドモード |

詳細は [`docs/usecases.md`](usecases.md) と
[`docs/decisions/0006-auto-record-and-multiview.md`](decisions/0006-auto-record-and-multiview.md)
を参照。

## 技術スタック詳細

### 言語・フレームワーク

| レイヤ            | 採用                          | 理由                                                                 |
| ----------------- | ----------------------------- | -------------------------------------------------------------------- |
| アプリ基盤        | Tauri 2.x                     | バイナリが小さく (~5–15MB)、ネイティブに近い性能、3 OS 対応         |
| HTTP サーバ       | axum 0.7                      | tokio + tower エコシステム、Service Worker 配信に充分軽量            |
| バックエンド言語  | Rust (stable)                 | メモリ安全、HTTP/バイナリ処理に強い、async/await が成熟              |
| HTTP クライアント | reqwest                       | async 対応、cookie/プロキシ対応                                      |
| 文字コード変換    | encoding_rs                   | Shift_JIS/EUC-JP/UTF-8 を網羅、Web 標準準拠                          |
| HTML サニタイズ   | ammonia                       | BBS HTML レンダリングモード用、whitelist ベース                      |
| 設定保存          | serde + toml                  | 人間可読、差分が取りやすい (TOML 単一、JSON は使わない)              |
| 動画再生 (Desktop)| libmpv (Rust FFI、`libmpv2`)  | 全 OS 対応、コーデック内蔵、低レイテンシ                             |
| 動画再生 (Web)    | hls.js + ネイティブ HLS       | Safari はネイティブ、その他は hls.js                                 |
| フロントエンド (Desktop) | Svelte 5 + SvelteKit + TypeScript + Vite | 軽量、コンパイル時に最適化される (ADR-0003) |
| フロントエンド (pst-server) | Vanilla JS (PWA) | Service Worker + manifest だけで十分な軽量 SPA |

### 動画再生バックエンドの選定方針

PeerCast から流れてくるストリームは主に **FLV / MPEG-2 TS / WMV** などの
コンテナ。Windows 版 PCRPlayer は DirectShow + 自作 FLVSplitter + EVR
Custom Presenter を組んでいたが、これは Windows 専用。

クロスプラットフォーム要件下では:

- **Desktop**: libmpv に統一。OpenGL/D3D11/Metal の各レンダラを持ち、
  Tauri のウィンドウハンドルを `wid` プロパティで渡してネイティブに
  埋め込む。Rust バインディングは [`libmpv2`](https://crates.io/crates/libmpv2)
- **Web (pst-server)**: PeerCastStation の HLS 出力 (`/hls/{id}.m3u8`) を
  pst-server がプロキシし、ブラウザ側で hls.js (or Safari ネイティブ)
  で再生。プロキシ層はディスクに書かず `bytes_stream()` で透過 (SD カード
  保護方針)

## モジュール構成

### Cargo workspace

```
pstplayer/                  ← workspace root (Cargo.toml)
├── crates/
│   ├── pst-core/           ← UI 非依存の共通ロジック (library crate)
│   └── pst-server/         ← axum HTTP API + 静的 PWA (binary)
└── src-tauri/              ← Tauri デスクトップアプリ (binary)
```

### pst-core (共通ロジック)

```
crates/pst-core/src/
├── lib.rs                  ← re-export ハブ
├── cli.rs                  ← CLI 引数パース (位置引数 + 長オプション)
├── single_instance.rs      ← channel_id 単位ロック + TCP IPC
├── snapshot.rs             ← 録画/スナップショットの出力先解決 + ファイル名生成
├── favorites.rs            ← ルール評価 (first match) + 互換 color
├── peercast/
│   ├── client.rs           ← 戦略選択 (JSON-RPC 優先 → legacy 落ち)
│   ├── jsonrpc.rs          ← /api/1 (JSON-RPC 2.0)
│   ├── legacy_admin.rs     ← /admin?cmd=viewxml (quick-xml)
│   ├── playlist.rs         ← .pls / .m3u パーサ
│   ├── types.rs            ← ChannelId / ChannelInfo / Track / Status 等
│   ├── url.rs              ← URL パーサ (pls / stream / play.html 三形式)
│   └── yp.rs               ← index.txt パーサ + 並行 fetch (JoinSet)
├── bbs/
│   ├── classify.rs         ← URL → BoardKind ルータ
│   ├── shitaraba.rs        ← したらば JBBS (rawmode / write.cgi)
│   ├── ch2.rs              ← 2ch 互換 (dat / bbs.cgi、Cookie 2 段階)
│   ├── encoding.rs         ← Shift_JIS / EUC-JP / UTF-8 + HTML エンティティ
│   ├── parse.rs            ← subject.txt / dat / setting.txt パーサ
│   ├── anchor.rs           ← >>N, ID 抽出
│   ├── sanitize.rs         ← ammonia HTML サニタイザ
│   └── types.rs            ← BoardInfo / ThreadInfo / Post 等
├── config/
│   ├── load.rs             ← TOML 読み書き + 破損時 .bak 保護
│   └── schema.rs           ← serde 構造体 (PeerCast / BBS / Player / Hub …)
└── util/
    ├── http.rs             ← reqwest クライアント共通設定 (User-Agent)
    └── errors.rs           ← thiserror ベースの AppError + IpcError
```

### pstplayer (Tauri デスクトップ)

```
src-tauri/src/
├── lib.rs                  ← Tauri Builder 組み立て、single_instance 取得、focus listener
├── main.rs                 ← lib::run() を呼ぶだけ
├── channel_polling.rs      ← バックエンド側のチャンネル状態ポーラー (5秒間隔)
├── commands/
│   ├── peercast.rs         ← resolve_stream_url / fetch_* / spawn_viewer / list_active_viewers 等
│   ├── bbs.rs              ← list_threads / fetch_thread / post_to_thread
│   ├── player.rs           ← player_load / pause / volume / snapshot / record_*
│   ├── config.rs           ← get_config / set_config / 履歴 / window_geometry
│   └── cli.rs              ← get_cli_args / resolve_default_endpoint
└── player/
    ├── engine.rs           ← libmpv2 ラッパー (load / pause / property / screenshot / stream-record)
    ├── window.rs           ← OS 別ウィンドウハンドル取得 (Win32 / AppKit / X11)
    └── mod.rs
```

### pst-server (常駐 HTTP)

```
crates/pst-server/src/
├── main.rs                 ← tokio runtime + bind + axum::serve
├── lib.rs                  ← Router 組み立て + AppState + AutoRecorder spawn
├── config.rs               ← pst-server.toml (peercast / server / recording / log)
├── handlers.rs             ← /api/* のハンドラ (channels / record / thread / config 等)
├── hls.rs                  ← /hls/{id}.m3u8 / /hls/{id}/{seg} プロキシ
├── recording.rs            ← RecordingState (HashMap<id, JoinHandle>)
├── auto_record.rs          ← favorites + getChannels polling → 自動 start/stop
├── error.rs                ← ApiError (HTTP status + code + message)
└── state.rs                ← AppState (cfg + recording + http client)
```

加えて `crates/pst-server/web/` 配下に Vanilla JS の PWA (manifest.json /
service-worker.js / app.js / settings.js / vendor/hls.min.js 等) が
あり、`/` で配信。

### フロントエンド (Desktop)

```
src/                        ← SvelteKit プロジェクト
├── routes/
│   ├── +page.svelte        ← ビューア画面 (動画 + BBS ペイン)
│   ├── hub/+page.svelte    ← ハブ画面 (PeCaRecorder 風テーブル)
│   ├── settings/+page.svelte ← 設定ダイアログ (タブ構成)
│   └── (その他: threads / channel-info / yp 等のサブウィンドウ)
└── lib/
    ├── api.ts              ← Tauri command の型付きラッパ
    ├── format.ts           ← 表示用フォーマッタ (時間 / ビットレート / 日付)
    └── (その他)
```

## 通信パターン

### Desktop ハブ → ビューア (プロセス間 IPC)

ハブから「視聴」を実行すると `pstplayer.exe <url>` を `Command::spawn`。
ビューアは起動時に single_instance ロックを取り、TCP listener を立てる。
ハブはロックファイルから `ipc_addr` を読み、以下の opcode を送れる
(各 opcode は `ok\n` 応答を返す):

| opcode | 用途 |
| ------ | ---- |
| `ping\n`     | 生死確認。`pong\n` が返る (probe_alive) |
| `focus\n`    | ウィンドウを前面化 |
| `close\n`    | プロセス終了 (一括閉じ機能) |
| `state\n`    | 録画中なら `1\n`、そうでなければ `0\n` |
| `startrec\n` | 録画開始 (viewer 側で path 構築 + libmpv stream-record 設定) |
| `stoprec\n`  | 録画停止 |

### Frontend → Backend (Tauri command)

```ts
// 例: チャンネル URL を再生
const streamUrl = await invoke<string>('resolve_stream_url', { url: '...' });
await invoke('player_load', { url: streamUrl });
```

### Backend → Frontend (Tauri event)

ポーリングではなくイベント push。

- `channel:status` — チャンネル状態の定期更新 (5 秒間隔)
- `yp:selected` / `thread:selected` — サブウィンドウからメインへの選択イベント
- `config:saved` — 設定保存時 (各画面が即時反映)

### pst-server → ブラウザ

通常の HTTP / fetch。Service Worker でオフラインフォールバック (PWA)。
動画は `<video>` 要素に HLS playlist URL を渡して hls.js or ネイティブ
HLS で再生。

## 状態管理

### Desktop

- **永続化**: 設定全般、ウィンドウ位置 / サイズ、視聴履歴、recent_hosts、
  お気に入りルール、YP ソース一覧 → `config.toml`
- **セッション**: 現在のチャンネル情報、現在の PeerCast 接続先 (CLI 引数
  由来かもしれない)、libmpv エンジン → Tauri State (`Arc<Mutex<…>>`)
- **フロントローカル**: UI 状態 (タブ選択 / フィルタ文字列 / 通知済み ID
  リスト) → Svelte ストア + localStorage

### pst-server

- **永続化**: `pst-server.toml` (peercast 接続先 / bind / recording / log /
  favorites)
- **メモリのみ**: `RecordingState (Mutex<HashMap<id, Task>>)`, `cfg
  (RwLock<Config>)`

### PeerCast 接続先の解決ロジック (Desktop)

`AppState` 内に `current_peercast: PeerCastEndpoint` を持ち、起動時 /
CLI 受信時に以下の優先順位で解決:

```
1. CLI 引数 URL に含まれる host:port → セッション限定で採用、config に書かない
2. config.toml の [peercast] host/port → 通常時はこれ
3. ハードコード default = localhost:7144
```

外部ツールから渡された URL のホストが LAN 内別マシンを指していても、
現在のセッションでは設定値ではなくその URL のホストを使う。

## エラー処理方針

- Rust 内部: `thiserror` でドメイン別エラー型 (`AppError`)。`PeerCastUnreachable`
  / `ThreadGone` / `BoardRegulated` / `PostRejected` 等のバリアントで分類
- IPC 境界: `String` ではなく構造化 (`IpcError { code, message, details }`)
  で返す。フロントで i18n / 個別ハンドリングしやすくする
- 起動時の疎通チェック: `peercastPing()` (getVersionInfo) → 失敗なら設定
  ウィンドウを自動オープン + `lastError` 表示
- TOML config 破損時: `.bak.YYYYMMDD_HHMMSS` を作ってデフォルト起動

## セキュリティ考慮

- **Tauri CSP**: `tauri.conf.json` で `csp` を厳格に設定 (script-src 'self'
  / connect-src 'self' http://localhost:* / フォント・画像のみ inline 許可)
- **HTTP リクエストは Rust 経由**: フロントから直接 `fetch` させない。
  Tauri command 越しに reqwest クライアントが投げる
- **BBS への書き込みは確認ダイアログ必須**: フロント側で `confirm()`
- **HTML サニタイズ**: BBS HTML レンダリングモードは ammonia で
  `script` / `iframe` / `style` / inline event handler / `javascript:` を全削除
- **YP URL の SSRF 対策**: `fetch_index` は `http://` / `https://`
  以外のスキームを拒否
- **channel_id 検証**: `spawn_viewer` は `ChannelId::parse()` (32 桁 hex 必須)
  で URL injection / path traversal を防止
- **HLS プロキシ**: id / segment に `..` / `/` / `\` / 制御文字を含む
  リクエストを 400 で拒否
- **single_instance ロック**: ASCII 英数のみのファイル名に正規化 (path
  traversal 対策)。空文字に潰れた場合は `ch-invalid.lock` に逃がして衝突防止
- **pst-server**: 現状認証なし。LAN 内利用前提 (`docs/usage/server.md`
  に明記)。WAN 公開時の認証は将来課題

## ビルド・配布

- 開発 (Desktop): `npm run tauri dev`
- 開発 (pst-server): `cargo run -p pst-server`
- ビルド (Desktop): `npm run tauri build` → 各 OS のパッケージ
- ビルド (pst-server): `cargo build -p pst-server --release`
- CI: GitHub Actions のマトリクスで 3 OS lint + test (`ci.yml`)、main push と
  手動起動でリリースビルド (`build.yml`) + Artifacts として 14 日間保持
- 詳細は [`docs/release.md`](release.md) を参照
