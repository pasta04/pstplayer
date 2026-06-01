# 開発ロードマップ

PSTPlayer の段階的な開発計画。各フェーズで「動くもの」を出すことを優先し、機能を縦に積み上げる。

## フェーズ 0: 設計 (完了)

- [x] 技術スタック決定 (Tauri + Rust + Web UI)
- [x] ライセンス方針決定 (MIT、クリーンルーム)
- [x] PCRPlayer 機能の棚卸し
- [x] PeerCast / BBS プロトコル仕様の調査
- [x] アーキテクチャ・機能・プロトコル各ドキュメント作成
- [x] フロントエンド UI フレームワーク決定 (Svelte 5)
- [x] 設計レビュー (重大度: 高 5 件解消、中 4 件解消)

## フェーズ 1: 最小プロトタイプ (MVP, v0.1)

**ゴール**: PeerCast の URL を渡せば動画が再生でき、コンタクト URL の したらば スレが読める。

### 1.1 プロジェクトスケルトン

- [x] `cargo create-tauri-app` でプロジェクト生成 (svelte-ts テンプレート)
- [x] Tauri 2.x の設定 (CSP 厳格化、ウィンドウ最小サイズ、identifier)
- [x] フロントエンドフレームワーク (Svelte 5 + SvelteKit + TypeScript)
- [x] CI セットアップ (GitHub Actions matrix: Ubuntu/macOS/Windows + frontend)
- [x] ESLint / Prettier / rustfmt / clippy 設定 (リリースプロファイル最適化含む)
- [x] テスト実行の枠 (`cargo test` で 31 件 pass。vitest はフロント追加時に検討)

### 1.2 PeerCast 連携 (最小)

- [x] `peercast` モジュール骨組み (`reqwest` ベース、`util::http` 共通クライアント)
- [x] URL パーサ (`pls`/`stream`/`play.html` 三形式 + `?tip=` パラメータ対応、IPv6 host も)
- [x] `/pls/{id}` から実ストリーム URL を取得 (PLS/M3U 両対応 + BOM 対応)
- [x] `/api/1` JSON-RPC クライアント (`getVersionInfo`, `getChannels`, `getChannelInfo`, `getChannelStatus`, `bumpChannel`, `stopChannel`)
- [x] 旧 `/admin?cmd=viewxml` フォールバック (XML パース、quick-xml)
- [x] 戦略選択 (JSON-RPC 優先 → legacy_admin フォールバック、`client::fetch_info` 等)
- [x] 単体テスト 43 件 pass (URL 6 / playlist 6 / YP 2 / encoding 5 / anchor 5 / cli 4 / config 1 / jsonrpc 3 / legacy_admin 3)
- [x] Tauri command 公開 (`resolve_stream_url`, `endpoint_for_url`, `fetch_channel_info`, `fetch_channel_status`, `bump_channel`, `stop_channel`)
- [x] **CLI 引数からの URL 受け取りで再生開始** (`get_cli_args` Tauri command 経由、`--no-autoplay` 尊重)
- [x] **PeerCast 接続先 (host/port) を設定 UI で指定可能に** (settings ダイアログ PeerCast タブ)
- [x] **Basic 認証情報の受け渡し** (`endpoint_for_url` が config の auth を endpoint に付加 → JSON-RPC / legacy admin で basic_auth を送出)
- [x] **接続先の優先順位解決ロジック** (`client::resolve_endpoint` で CLI URL > config > localhost:7144、`resolve_default_endpoint` Tauri command で公開)

### 1.3 動画再生 (最小)

- [x] libmpv の Rust バインディング選定・導入 (`libmpv2` v5)
- [x] PlayerEngine 実装 (load / stop / pause / volume / mute / property get)
- [x] Tauri State として `setup` フックで初期化、Linux で初期化テスト pass
- [x] Tauri ウィンドウへの埋め込み (`wid` プロパティ、Win32/AppKit/Xlib/Xcb
      対応、Wayland は明示エラー、`player_attach` コマンドで window label
      を指定して呼び出し可)
- [x] フロントから操作するコマンド (player_load / player_stop / player_set_*
      / player_status / player_attach を公開、TS ラッパーも追加)

### 1.4 BBS 連携 (最小: したらば のみ)

- [x] EUC-JP / Shift_JIS / UTF-8 変換 (`encoding_rs`) + HTML エンティティ復号
- [x] したらば `subject.txt` パース (カンマ区切り、6 フィールド dat 対応)
- [x] 2ch 互換 `subject.txt` パース (`<>` 区切り、7 フィールド dat 対応)
- [x] アンカー (`>>N`, `>>N-M`, 全角 `＞＞`) 抽出
- [x] URL → BoardKind ルータ (router.rs)
- [x] HTTP クライアント (Cookie 対応、reqwest cookie_store。永続化はフェーズ 2 で)
- [x] したらば `rawmode.cgi` での実取得 + `If-Modified-Since` 差分取得
- [x] したらば `write.cgi` への POST (確認ダイアログはフロント側 TODO)
- [x] 2ch 系 dat 取得 + `Range` 差分取得 (304/416 ハンドリング含む)
- [x] 2ch 系 `bbs.cgi` 投稿 (Cookie 2 段階確認自動再送)
- [x] ammonia による HTML サニタイズ (HTML 表示モード用)
- [x] Tauri command 公開: `classify_board`, `list_threads`, `fetch_thread`,
      `post_to_thread`, `sanitize_html`
- [x] URL 末尾サフィックス (`/l30`, `/501-1000` 等) を許容して全レス / 差分取得が機能
      (pst-core パーサがサフィックスを無視 + フロントが canonical `/{key}/` に正規化)

### 1.5 UI (最小)

- [x] メインウィンドウ (動画 + サイドバー BBS の 2 ペイン)
- [x] URL 貼り付けで再生開始 (player_attach で libmpv をウィンドウに埋め込み、
      player_load(streamUrl) で実再生開始。stop / pause も結線済)
- [x] チャンネル情報の表示 (名前・ジャンル・詳細)
- [x] レス一覧の表示 (プレーン / HTML 切替可)
- [x] 書き込みフォーム

### 1.6 設定

- [x] OS 標準 config ディレクトリへ `config.toml` 保存
- [x] ウィンドウ位置・サイズの保存/復元 (メインのみ、physical px、500ms debounce)
- [x] 名前・メールデフォルト

**完了条件**: したらばコンタクト URL のチャンネルを 3 OS で視聴 + 書き込みできる。

---

## フェーズ 2: 機能充実 (v0.5)

**ゴール**: PCRPlayer の主要機能を網羅し、日常使いに耐える。

### 2.1 PeerCast

- [x] YP チャンネル一覧 (`index.txt` パース、19 フィールド、別ウィンドウで取得 →
      ソート可能なテーブル → 行クリックで `yp:selected` event → メインで自動再生。
      取得先 URL は設定 → PeerCast → 「YP index.txt URL」で指定)
- [x] リレー状況・接続情報の詳細表示 (右クリック → 📊 チャンネル詳細、
      ChannelInfo/Status/PlayerStat の全フィールドをモーダル表示)
- [x] チャンネル情報の定期更新 (pseudo-push: backend の tokio タスクが
      5 秒間隔で fetch_status → `channel:status` event でフロントに配る。
      複数 listener (詳細モーダル / 将来の追加ウィンドウ) で共有可能)

### 2.2 BBS

- [x] 2ch 系 (5ch / jpnkn 等) 対応 (`<>` 7 フィールド、Shift_JIS。`pst-core::bbs::ch2`)
- [x] 自動更新 (`If-Modified-Since` + `Range: bytes=N-` で差分取得、5 秒間隔タイマー)
- [x] 同一 ID 抽出表示 (ID クリックでポップアップ + フィルタ欄 `id:xxx` で持続抽出)
- [x] レス番号での抽出 (フィルタ欄 `>>123` / `>>120-130` で前後 2 件も含めて表示)
- [x] キーワード検索 (フィルタ欄、Ctrl+F でフォーカス、本文/名前/ID/番号横断)
- [x] URL 自動リンク化 (本文中の http(s) を `<a class="external">` 化、クリックで OS 既定ブラウザを起動)
- [x] 新着レス通知 (OS 標準通知) — `notifyOnNewPost` 隠しオプション、既定 OFF
- [ ] HTML レンダリング表示モード (スキン適用、CSS で装飾) — **優先度: 低**
- [x] 新着レス到着時の自動スクロール (既定 ON、手動スクロール中は一時停止、設定 UI で OFF 可)
- [x] スレッドが落ちた時の検知 (404/410/Not Found を判定、💀 表示 + 自動更新停止、
      Ctrl+Shift+R で再試行)

### 2.3 プレイヤー

- [x] アスペクト比切替 (7 種、Alt+1-7 / 右クリック → アスペクト比 ▶)
- [x] サイズプリセット (9 段階、Ctrl+1-9 / 右クリック → サイズ ▶)
- [x] 全画面切替 (F11 / ダブルクリック / 右クリックメニュー、Tauri `setFullscreen`)
- [x] 常に最前面 (ホットキー T + 右クリックメニュー、✓ で現在状態を表示)
- [x] スナップショット (F2 / 右クリック、1 秒連発時はトーストを最後の 1 件にまとめる)
- [x] ボリュームのホイール操作 (動画エリア上で wheel、step 5、0-150%)

### 2.4 UI

- [x] ショートカット (デフォルトセット + カスタマイズ画面: 設定 → ショートカット
      タブで各 action のキーを変更 / 無効 / 初期化可。Esc キャプチャキャンセル、
      衝突検出 + 警告表示。Ctrl+1〜9 / Alt+1〜7 / Esc は固定)
- [x] 右クリックメニュー (動画エリアで再接続 / 全画面 / 常に最前面 / コンタクト URL /
      チャンネル詳細 / サイズ / アスペクト / 視聴履歴 / スナップショット / 録画 /
      設定 / スレ一覧 / YP の各エントリ。サイズ・アスペクト・履歴はサブメニュー)
- [x] ダーク/ライトテーマ (一般タブから切替、CSS 変数で全画面に即時反映)
- [x] BBS ペインの表示/非表示 (C キーで切替、設定 UI でも反映。位置切替は固定右ペイン)
- [x] 視聴履歴 (最大 30 件、右クリック → 🕒 視聴履歴 ▶ で再アクセス、設定 → 履歴 タブ
      で一覧 / クリア)
- [x] **設定ダイアログ** (別ウィンドウ、PeerCast/BBS タブ最低限実装、TOML 永続化に接続)
- [x] **スレ一覧ウィンドウ** (別ウィンドウ、subject.txt 取得 + 更新ボタン +
      行クリックで `thread:selected` event をメインに発火)

### 2.5 配布

- [x] **配布方針ドキュメント** (`docs/release.md` — バージョニング / タグ運用 /
      配布物命名 / アイコン仕様 / CI ワークフローとの対応 / 将来計画)
- [x] アイコン設計 (PSTPlayer 専用、メガホン + 放射波のシルエット、`tauri icon` で
      全 OS 用一括生成。iOS/Android/Microsoft Store 用は `.gitignore` で除外)
- [x] Windows portable ZIP / macOS DMG/APP / Linux DEB/AppImage/RPM の生成
      (`.github/workflows/build.yml`、3 OS で `npm run tauri build`、
      Artifacts として 14 日間保持。Windows は NSIS 不安定のため portable に切替済)
- [x] GitHub Releases への自動発行 (`v*` タグトリガで build.yml に release ジョブ追加、
      `pstplayer-{version}-{os}-{arch}.{ext}` 形式にリネーム + `SHA256SUMS.txt` 添付、
      `-rc.* / -beta.* / -alpha.*` は自動で prerelease 扱い)
- [x] Windows の libmpv-dev リンク安定化 (`continue-on-error` + `matrix.experimental`
      を撤去、`cargo test --no-run` での Windows スプリットも解消し全 OS で
      `cargo test --workspace` を実行)
- [x] `THIRD-PARTY.md` 生成 (`cargo-about` 設定 + テンプレート + release ジョブで都度生成、
      `staging/THIRD-PARTY-{version}.md` として Releases に同梱)

**完了条件**: PCRPlayer ユーザーがそのまま乗り換えられるレベルの基本体験。

---

## フェーズ 3: 安定化・品質 (v1.0)

- [x] エラー処理の網羅 (1 巡目: AppError を分類 + 起動時疎通チェック + 規制検出)
      - `AppError::{PeerCastUnreachable, ThreadGone, BoardRegulated, PostRejected}` 追加
      - `reqwest::Error::is_connect()/is_timeout()` で接続失敗を `PeerCastUnreachable` に分離
      - BBS 投稿応答 (`bbs::post_result`) で「ホスト規制 / プロバ規制 / !=BANNED!= 等」
        を `BoardRegulated`、その他拒否を `PostRejected` に分類
      - `config::load_or_default()` で TOML 破損時に `.bak.YYYYMMDD_HHMMSS` を作って
        デフォルト起動。アプリが立ち上がらない状態を防ぐ
      - 起動時に `peercastPing()` (getVersionInfo) → 失敗なら設定ウィンドウを自動オープン
        + lastError 表示
      - 残: libmpv 再生中の stream 切断検知、ネットワーク断時の自動 retry など
- [ ] パフォーマンス計測と改善 (起動時間、メモリ、CPU) — **優先度: 低**
- [ ] 3 OS で実機 QA — **優先度: 低**
- [ ] バグ修正 — **優先度: 低**
- [x] ユーザマニュアル (`docs/usage/` 1 巡目: README + install + first-setup +
      basic + shortcuts + troubleshooting の 6 章。スクリーンショットは実機 QA 時に追加)
- [ ] (任意) 自動更新機構 (Tauri Updater) — **優先度: 低**

**完了条件**: v1.0 タグ。リリースバイナリを 3 OS で配布。

---

## フェーズ 4: 拡張 (v1.0 以降)

- [x] 録画 (Desktop + pst-server 両方の MVP)
      - Desktop: libmpv の `stream-record` で再エンコード無しに保存。
        右クリック → ⏺ 録画開始 / ⏹ 録画停止。設定 → プレイヤー で
        保存先ディレクトリと拡張子を指定 (空なら exe 配下 `recordings/`,
        拡張子は `flv`)。お気に入りに `auto_record = true` のルールが
        あれば視聴開始時に自動録画
      - pst-server: `/api/record/{start,stop,list}` を追加 (複数本並行)。
        tokio task が上流 `/stream/{id}.{ext}` を bytes_stream →
        tokio::fs::File に書き出し。`[recording] enabled=true, dir="..."`
        時のみ有効、`max_concurrent` (既定 8) まで同時録画可
      - Web (`/`): 視聴中に ⏺ 録画 ボタン (list API で機能の有無 + 現
        チャンネルの状態を判定)。お気に入り auto_record で視聴開始時に
        自動録画
      - 動作確認後に使い勝手 (ファイル名 / 出力形式 / 自動停止条件)
        を仕様調整予定
- [x] **お気に入り機能** (`pst-core::favorites`)
      - 設定 → お気に入り タブで複数ルールを編集 (名前 / チャンネル名
        / ジャンル / 詳細 / コメント、上位固定、自動録画、背景色)。
        フィールドは部分一致 (大文字小文字無視) で空欄ワイルドカード、
        複数記述で AND。並び順が優先順位
      - Desktop YP ウィンドウ / pst-server Web フロントの両方でルールに
        マッチした行を上位固定 + 背景色 + ★ マークで表示
      - pst-server は `/api/favorites` で config の `[[favorites.rules]]`
        を JSON で返す (read-only)
      - 視聴開始時に `auto_record = true` ルールにマッチすれば自動録画

### 4.A 高優先度 — 自動 / 並行系の拡張

- [ ] **自動配信録画 (お気に入りで配信開始を自動検知)** ← **優先度: 高**
      - pst-server が定期的に紐付け先 PeerCastStation の `getChannels`
        (および任意で YP) を polling
      - 出現したチャンネルが `[[favorites.rules]]` に `auto_record = true`
        でマッチしたら自動で録画開始 (現状は人が視聴開始した時のみ)
      - チャンネルが消えたら自動で録画停止 (現状は手動 stop のみ)
      - max_concurrent / dir 等は既存の `[recording]` を継承
      - 詳細設計: [ADR-0006](decisions/0006-auto-record-and-multiview.md)
- [ ] **複数チャンネル視聴アーキテクチャ** ← **優先度: 高**
      - Desktop: 現状 libmpv 1 本制約 → ウィンドウ複製 or 1 ウィンドウ
        内のタイル表示で複数チャンネル同時視聴
      - pst-server Web: グリッド表示で `<video>` × N、各々独立に HLS
        再生 (帯域 / CPU 負荷の上限はユーザー裁量)
      - お気に入り / YP からまとめて開く UX
      - 詳細設計: [ADR-0006](decisions/0006-auto-record-and-multiview.md)

### 4.B その他

- [ ] プラグイン機構 (BBS タイプ追加、フィルタ等) — **優先度: 低**
- [ ] 英語 UI (i18n) — **優先度: 低**
- [ ] PeerCast 本体の自動起動・終了 — **優先度: 低**
- [ ] **リレーサーバ `pst-server`** (iPad/iPhone/Android からブラウザで視聴 + 投稿。
      設定ファイルに紐付け先 PeerCastStation を書き、`getChannels` をプロキシして
      TOP ページに視聴可能チャンネル一覧を表示、HLS 出力で全端末対応、PWA 化。
      詳細は [ADR-0005](decisions/0005-workspace-and-server.md))
      - [x] **MVP 着手**: `crates/pst-server/` 作成、axum 0.7 ベース
            - [x] `pst-server.toml` 設定 (peercast 接続先 + bind / public_url)
            - [x] PeerCast API プロキシ: `GET /api/channels`, `GET /api/channel/{id}/info`,
                  `/status`, `POST /api/channel/{id}/bump`, `/stop`
            - [x] YP プロキシ: `GET /api/yp?url=...`
            - [x] BBS プロキシ: `GET /api/board?url=...`, `GET /api/thread?url=...`,
                  `POST /api/thread/post`
            - [x] エラーは pst-core::AppError を HTTP status にマッピング
                  (peercast_unreachable → 502, board_regulated → 403, 等)
            - [x] `--config <path>` で設定パス指定可、無ければ OS 標準
      - [x] **静的フロント (Vanilla JS PWA) を `/` でホスト + manifest + Service Worker**
            (Svelte は使わず軽量 SPA、ServeDir でフォールバック配信。
            `--web <dir>` / `PST_SERVER_WEB_DIR` / exe 隣の `web/` / 開発時のソース
            ツリーの順で解決)
      - [x] **HLS プロキシ** (PeerCastStation の `/hls/{id}` を `bytes_stream` で
            透過、SD カード保護のためディスク書き込み無し。Range / If-Modified-Since
            は上流に転送、Basic 認証も付加)
      - [x] **SD カード保護のログ仕様** (既定 OFF、`[log] debug = true, dir = "..."`
            時のみ tracing-appender で日次ローテーション)
      - [ ] **認証 / 公開モード** (LAN 外公開時の Basic / OAuth 等) — **優先度: 低**
- [ ] リモート操作 (別端末からの再生制御) ← 上記サーバの延長 — **優先度: 低**
- [ ] `pstplayer://` カスタム URL スキーマ (ブラウザからのワンクリック起動) — **優先度: 低**

---

## 進捗指標

各フェーズで、以下を Done の条件とする:

1. **動く**: 3 OS で起動して該当機能が使える
2. **書かれている**: 該当機能の使い方が docs に記載されている
3. **テストがある**: 主要パスにテスト (Rust unit / フロント component / E2E のいずれか)
4. **CI が緑**: 全 OS のビルドが通っている

## リスク

| リスク                                            | 影響度 | 対応                                                        |
| ------------------------------------------------- | ------ | ----------------------------------------------------------- |
| libmpv のウィンドウ埋め込みが OS 間で安定しない   | 高     | 早期 PoC、必要なら libvlc 代替検討                          |
| BBS 側の規制変更で投稿が通らなくなる              | 中     | ユーザエージェント・Cookie をユーザ設定可能に               |
| PeerCast 本体が JSON-RPC 未対応の旧版の場合       | 中     | `/admin?cmd=` 互換層を MVP から含める                       |
| Tauri 2.x の API 変更                             | 低     | バージョン固定、`cargo update` 慎重に                       |
