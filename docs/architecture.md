# アーキテクチャ

PSTPlayer の全体構成、モジュール分割、外部依存関係をまとめる。

## 全体像

```
┌─────────────────────────────────────────────────────────┐
│  Frontend (Web UI, TypeScript)                          │
│                                                         │
│  [Main Window]                                          │
│  ┌──────────────────────────┬───────────────────────┐  │
│  │ Player View              │ BBS Pane              │  │
│  │ (libmpv の描画領域)      │ (レス一覧 + 書き込み) │  │
│  │  + ステータス帯          │                       │  │
│  └──────────────────────────┴───────────────────────┘  │
│                                                         │
│  [Sub Windows] (Tauri マルチウィンドウ)                 │
│   - スレ一覧ウィンドウ (更新ボタン + スレ選択)          │
│   - 設定ダイアログ (タブ構成)                           │
│   - チャンネル詳細ダイアログ                            │
└───────────────────────────┬─────────────────────────────┘
                            │ Tauri IPC (command / event)
┌───────────────────────────┴─────────────────────────────┐
│  Backend (Rust, Tauri Core)                             │
│  ┌──────────────┬──────────────┬──────────────────────┐ │
│  │ cli          │ single_inst  │ commands (IPC)       │ │
│  │ (引数パース) │ (多重起動制御)│                      │ │
│  └──────────────┴──────────────┴──────────────────────┘ │
│  ┌──────────────┬──────────────┬──────────────────────┐ │
│  │ peercast     │ bbs          │ player (libmpv FFI)  │ │
│  │ (HTTP API)   │ (HTTP + 文字 │ (再生制御 / 状態)    │ │
│  │              │  コード変換) │                      │ │
│  └──────────────┴──────────────┴──────────────────────┘ │
│  ┌──────────────┬──────────────┬──────────────────────┐ │
│  │ config       │ history      │ cookie_jar           │ │
│  │ (TOML)       │ (視聴履歴)   │ (BBS Cookie 永続化)  │ │
│  └──────────────┴──────────────┴──────────────────────┘ │
└───────────────────────────┬─────────────────────────────┘
                            │
        ┌───────────────────┴──────────────────┐
        │                                      │
   PeerCast (host:port は可変、既定 7144) BBS サーバ
   - /api/1 (JSON-RPC 2.0)               - したらば JBBS
   - /admin?cmd= (旧 API フォールバック) - 2ch 互換 (5ch 等)
   - /pls/{id}, /stream/{id}
```

- **CLI 引数経由起動** (features §2.1): 外部ツールから渡された URL のホスト/ポートに接続するため、`localhost:7144` 固定ではない
- **メインウィンドウは単一**、サブウィンドウ (スレ一覧 / 設定 / 詳細) は **Tauri のマルチウィンドウ機能** で別ウィンドウとして開く
- 設定ファイル形式は **TOML 単一**、JSON は使わない

## 技術スタック詳細

### 言語・フレームワーク

| レイヤ            | 採用                          | 理由                                                                 |
| ----------------- | ----------------------------- | -------------------------------------------------------------------- |
| アプリ基盤        | Tauri 2.x                     | バイナリが小さく (~5–15MB)、ネイティブに近い性能、3 OS 対応         |
| バックエンド言語  | Rust (stable)                 | メモリ安全、HTTP/バイナリ処理に強い、async/await が成熟              |
| HTTP クライアント | reqwest                       | async 対応、cookie/プロキシ対応                                      |
| 文字コード変換    | encoding_rs                   | Shift_JIS/EUC-JP/UTF-8 を網羅、Web 標準準拠                          |
| HTML/正規表現     | scraper, regex                | BBS スクレイピング用                                                 |
| 設定保存          | serde + toml                  | 人間可読、差分が取りやすい (TOML 単一、JSON は使わない)              |
| 動画再生          | libmpv (Rust FFI)             | 全 OS 対応、コーデック内蔵、低レイテンシ                             |
| フロントエンド    | TypeScript + (Svelte / React) | 軽量、Tauri 公式テンプレート充実 (どちらを選ぶかは ADR-0003 で別途) |

### 動画再生バックエンドの選定方針

PeerCast から流れてくるストリームは主に **FLV / MPEG-2 TS / WMV** などのコンテナ。Windows 版 PCRPlayer は DirectShow + 自作 FLVSplitter + EVR Custom Presenter を組んでいたが、これは Windows 専用。

クロスプラットフォーム要件下では、内蔵する動画エンジンを 1 つに統一するのが現実的:

- **libmpv (採用候補本命)**: mpv 内蔵 (FFmpeg ベース)。HTTP ストリーミング、FLV/TS/WMV を含むほぼ全てのフォーマットに対応。OpenGL/D3D11/Metal の各レンダラを持つ。Rust バインディング ([`libmpv2`](https://crates.io/crates/libmpv2) 等) あり。
- **libvlc (代替候補)**: VLC の組み込みエンジン。libmpv より重いが、対応フォーマットも広い。

→ **libmpv を採用**。pcoplayer も libmpv 系のラッパー (mpv) を採用しているため、実装上の前例がある。

ウィンドウ埋め込みについては、Tauri WebView 上に重ねる方式 (mpv に専用ウィンドウを描画させ、Tauri のウィンドウハンドルを渡す) を取る。詳細は実装フェーズで詰める。

## モジュール構成 (Rust 側)

```
src-tauri/
├── Cargo.toml
├── tauri.conf.json
├── build.rs
└── src/
    ├── main.rs              # エントリ。Tauri Builder の組み立て
    ├── cli.rs               # コマンドライン引数パース (起動時 URL)
    ├── single_instance.rs   # 二重起動制御 (new-window/replace/single)
    ├── commands/            # Tauri command (フロントから呼べる API)
    │   ├── mod.rs
    │   ├── peercast.rs      # チャンネル取得、再生開始/停止、bump
    │   ├── bbs.rs           # スレ一覧、レス取得、書き込み
    │   ├── player.rs        # 再生制御、シーク、ボリューム
    │   └── config.rs        # 設定読み書き
    ├── peercast/            # PeerCast 通信ロジック (純粋関数中心)
    │   ├── mod.rs
    │   ├── client.rs        # HTTP API クライアント
    │   ├── jsonrpc.rs       # /api/1 (JSON-RPC 2.0)
    │   ├── legacy_admin.rs  # /admin?cmd= (旧形式、後方互換用)
    │   ├── playlist.rs      # .pls / .m3u パーサ → 実ストリーム URL
    │   └── types.rs         # ChannelInfo, Track, Relay, Status 等
    ├── bbs/                 # 掲示板ロジック
    │   ├── mod.rs
    │   ├── traits.rs        # BoardClient trait (掲示板タイプ共通 IF)
    │   ├── shitaraba.rs     # したらば JBBS 実装
    │   ├── ch2.rs           # 2ch 系 (dat ベース) 実装
    │   ├── encoding.rs      # Shift_JIS ↔ UTF-8、HTML エンティティ
    │   ├── parse.rs         # subject.txt / dat / setting.txt パーサ
    │   ├── anchor.rs        # >>N, ID 抽出
    │   └── types.rs         # BoardInfo, ThreadInfo, ResInfo 等
    ├── player/              # libmpv FFI ラッパ
    │   ├── mod.rs
    │   ├── engine.rs        # mpv 初期化、コマンド送信
    │   └── window.rs        # OS 別ウィンドウ埋め込み
    ├── config/              # 設定永続化
    │   ├── mod.rs
    │   └── schema.rs        # serde 構造体
    └── util/
        ├── http.rs          # reqwest クライアントの共通設定
        └── errors.rs        # thiserror ベースの統一エラー
```

## ディレクトリレイアウト全体

```
pstplayer/
├── README.md
├── LICENSE
├── docs/                    # 設計ドキュメント (このフォルダ)
├── src-tauri/               # Rust バックエンド
├── src/                     # フロントエンド (TypeScript)
│   ├── routes/              # 画面単位 (Svelte なら +page.svelte 等)
│   ├── components/
│   ├── lib/
│   │   ├── api.ts           # Tauri command の型付きラッパ
│   │   └── stores.ts        # 状態管理
│   └── styles/
├── package.json
└── .github/workflows/       # CI (build, lint, test)
```

## 通信パターン

### フロントエンド → バックエンド (Tauri command)

```ts
// 例: チャンネル URL を貼り付けて再生
await invoke('peercast_play', { url: 'http://localhost:7144/pls/abc...' });
```

Rust 側:

```rust
#[tauri::command]
async fn peercast_play(url: String, state: State<'_, AppState>) -> Result<(), String> {
    let stream_url = state.peercast.resolve_stream(&url).await?;
    state.player.load(&stream_url).await?;
    Ok(())
}
```

### バックエンド → フロントエンド (Tauri event)

ポーリングではなくイベント push。

- `channel:info_updated` — チャンネル情報更新
- `bbs:thread_updated` — スレッド更新
- `player:state_changed` — 再生状態変化
- `player:position` — 再生位置 (定期発火、頻度はフロントから調整)

## 状態管理

- **永続化が必要**: 設定 (window 配置、ホットキー、PeerCast デフォルト host:port、認証情報、最後のチャンネル等) → ファイル (`config.toml`)
- **セッション内のみ**: 現在のチャンネル情報、現在の PeerCast 接続先 (CLI 引数由来かもしれない)、スレ DAT、再生位置 → Rust 側 `AppState` (`Arc<Mutex<…>>`)
- **フロント側ローカル**: UI 状態 (タブ選択等)

### PeerCast 接続先の解決ロジック

`AppState` 内に `current_peercast: PeerCastEndpoint` を持ち、起動時/CLI 受信時に以下の優先順位で解決:

```
1. CLI 引数 URL に含まれる host:port → セッション限定で採用、config に書かない
2. config.toml の [peercast] host/port → 通常時はこれ
3. ハードコード default = localhost:7144
```

これにより、外部ツールから渡された URL のホストが LAN 内別マシンを指していても、現在のセッションでは設定値ではなくその URL のホストを使う動作になる。

## エラー処理方針

- Rust 内部: `thiserror` でドメイン別エラー型
- IPC 境界: `String` ではなく構造化 (`{ code, message, details }`) で返す。フロントで i18n 化しやすくする
- ネットワークエラーは UI に「リトライ」ボタンを出す

## セキュリティ考慮

- Tauri の `allowlist` で必要な API のみ許可
- 任意 URL の HTTP リクエストは Rust 側を経由 (フロントから直接 `fetch` させない)
- BBS への書き込みは確認ダイアログを必須に
- 外部からのコンタクト URL は表示前に正規化 (XSS 対策)

## ビルド・配布

- 開発: `cargo tauri dev`
- ビルド: `cargo tauri build` → 各 OS のパッケージ (`.msi`, `.dmg`, `.AppImage`/`.deb`)
- CI: GitHub Actions のマトリクスで 3 OS ビルド
- 署名: Windows (コード署名)、macOS (公証) は v1.0 までに対応検討

## 将来検討事項

- プラグイン機構 (BBS タイプの動的追加)
- リモート操作 (Web UI からの別端末視聴制御)
- 録画機能
