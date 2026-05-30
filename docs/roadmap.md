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
- [x] **CLI 引数からの URL 受け取りで再生開始 (外部ツール連携の基本)** - 引数パース完了
- [ ] **PeerCast 接続先 (host/port) を設定 UI で指定可能に** (localhost 以外: LAN 内別マシン対応)
- [ ] **Basic 認証情報の受け渡し** (config から取得して JSON-RPC リクエストに付加)
- [ ] **接続先の優先順位解決ロジック** (CLI 引数 > config > default)

### 1.3 動画再生 (最小)

- [x] libmpv の Rust バインディング選定・導入 (`libmpv2` v5)
- [x] PlayerEngine 実装 (load / stop / pause / volume / mute / property get)
- [x] Tauri State として `setup` フックで初期化、Linux で初期化テスト pass
- [ ] Tauri ウィンドウへの埋め込み (3 OS の `wid` プロパティ実装)
- [x] フロントから操作するコマンド (player_load / player_stop / player_set_*
      / player_status を公開、TS ラッパーも追加)

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

### 1.5 UI (最小)

- [ ] メインウィンドウ (動画 + サイドバー BBS の 2 ペイン)
- [ ] URL 貼り付けで再生開始
- [ ] チャンネル情報の表示 (名前・ジャンル・詳細)
- [ ] レス一覧の表示 (プレーン)
- [ ] 書き込みフォーム

### 1.6 設定

- [ ] OS 標準 config ディレクトリへ `config.toml` 保存
- [ ] ウィンドウ位置・サイズの保存/復元
- [ ] 名前・メールデフォルト

**完了条件**: したらばコンタクト URL のチャンネルを 3 OS で視聴 + 書き込みできる。

---

## フェーズ 2: 機能充実 (v0.5)

**ゴール**: PCRPlayer の主要機能を網羅し、日常使いに耐える。

### 2.1 PeerCast

- [ ] YP チャンネル一覧 (`index.txt` パース、19 フィールド)
- [ ] リレー状況・接続情報の詳細表示
- [ ] チャンネル情報の定期更新 (Tauri event 経由 push)

### 2.2 BBS

- [ ] 2ch 系 (5ch 等) 対応 (`<>` 7 フィールド、Shift_JIS)
- [ ] 自動更新 (`If-Modified-Since` + `Range: bytes=N-` で差分取得)
- [ ] 同一 ID 抽出表示
- [ ] レス番号での抽出
- [ ] キーワード検索
- [ ] URL 自動リンク化
- [ ] 新着レス通知 (OS 標準通知)
- [ ] HTML レンダリング表示モード (スキン適用、CSS で装飾)

### 2.3 プレイヤー

- [ ] アスペクト比切替
- [ ] サイズプリセット
- [ ] 全画面切替 (二重起動防止、ホットキー)
- [ ] 常に最前面
- [ ] スナップショット
- [ ] ボリュームのホイール操作

### 2.4 UI

- [ ] ショートカット (デフォルトセット + カスタマイズ画面)
- [ ] 右クリックメニュー
- [ ] ダーク/ライトテーマ
- [ ] BBS サブペインの表示/非表示・位置切替
- [ ] 視聴履歴

### 2.5 配布

- [ ] アイコン設計
- [ ] Windows MSI / macOS DMG / Linux AppImage の生成
- [ ] GitHub Releases への自動発行 (タグドリブン)

**完了条件**: PCRPlayer ユーザーがそのまま乗り換えられるレベルの基本体験。

---

## フェーズ 3: 安定化・品質 (v1.0)

- [ ] エラー処理の網羅 (ネットワーク断、サーバ規制、スレ落ち等)
- [ ] パフォーマンス計測と改善 (起動時間、メモリ、CPU)
- [ ] 3 OS で実機 QA
- [ ] バグ修正
- [ ] ユーザマニュアル (`docs/usage/`)
- [ ] (任意) コード署名 (Windows) / 公証 (macOS)
- [ ] (任意) 自動更新機構 (Tauri Updater)

**完了条件**: v1.0 タグ。リリースバイナリを 3 OS で配布。

---

## フェーズ 4: 拡張 (v1.0 以降)

- [ ] 録画 (`stream-record`)
- [ ] プラグイン機構 (BBS タイプ追加、フィルタ等)
- [ ] 英語 UI (i18n)
- [ ] PeerCast 本体の自動起動・終了
- [ ] **リレーサーバ `pst-server`** (iPad/iPhone/Android からブラウザで視聴 + 投稿。
      設定ファイルに紐付け先 PeerCastStation を書き、`getChannels` をプロキシして
      TOP ページに視聴可能チャンネル一覧を表示、HLS 出力で全端末対応、PWA 化。
      詳細は [ADR-0005](decisions/0005-workspace-and-server.md))
- [ ] リモート操作 (別端末からの再生制御) ← 上記サーバの延長
- [ ] `pstplayer://` カスタム URL スキーマ (ブラウザからのワンクリック起動)

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
