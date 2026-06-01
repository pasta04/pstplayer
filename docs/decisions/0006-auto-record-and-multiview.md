# ADR-0006: 自動配信録画 + 複数チャンネル視聴アーキテクチャ

- **ステータス**: 検討中 (2026-06-01) — 高優先度タスクの設計
- **決定者**: pasta04 (リポジトリオーナー)

## 背景

現状の制約と要件のズレ:

1. **録画は「人が視聴を開始した時」のみ発火する**
   - 自動配信録画 (`auto_record = true` のお気に入り) を満たすのは、
     人がチャンネルをクリックして視聴開始した直後の 1 回だけ
   - 「お気に入りチャンネルが配信を始めた時に勝手に録画開始」までは
     行かない (= 配信を見逃す可能性が常にある)
2. **同時視聴が 1 本に制約されている**
   - Desktop: libmpv エンジン 1 つ + メインウィンドウ 1 つで 1 配信
     のみ。複数チャンネルを同時に見るには複数アプリ起動が必要
   - pst-server Web: `<video>` × 1 + 単一 HLS フェッチで同様に 1 配信
     のみ。グリッド視聴ができない
3. **複数本録画は可能だが「複数本視聴」は未対応**
   - 録画は `RecordingState (HashMap<channel_id, Task>)` で max_concurrent
     まで並行可能 (4042e45 で対応済み)
   - 視聴側の並行性は手付かず

ユーザー要望:
- 「自動での複数配信録画」=「お気に入りに引っかかった配信が始まった
  瞬間に黙って録画する」
- 「複数チャンネル視聴に向けたアーキテクチャ検討」=「グリッドで N 本
  並べて見る」

両方を高優先度として、構造から手を入れる。

---

## 自動配信録画の設計

### A. polling ベース (本 ADR で採用)

**設計**:

- pst-server に新タスク **AutoRecorder** を 1 本常駐させる
- 30〜60 秒間隔で:
  1. 紐付け先 PeerCastStation の `getChannels` を fetch
  2. 各チャンネル情報を `firstFavoriteMatch(rules, info)` にかけて
     `auto_record == true` のヒットを抽出
  3. 既に録画中 (RecordingState に同 channel_id がある) なら skip
  4. 新規ヒットは `RecordingState::start(...)` を呼ぶ
- 配信が消えた (getChannels に居なくなった) チャンネルの録画は
  自動 stop。`grace = 60 秒`程度の猶予を持って一時的な切断と区別

**メリット**:

- 既存の `RecordingState` (HashMap, max_concurrent) を再利用するだけ
- 上流 PeerCastStation 側に push 機構が無いので polling は妥当
- max_concurrent で総量規制も既存設定で済む

**デメリット**:

- 配信開始の検知が 30〜60 秒遅れる (配信冒頭が欠ける可能性)
- 短い配信や瞬時の再接続は取り逃す
- YP 経由のチャンネルは紐付け先 PeerCastStation に流れて来ない限り
  検知できない

### B. PCP リレーで push 検知 (見送り)

PeerCast の PCP プロトコルでリレーに参加していれば push でチャンネル
リストを受け取れる。しかし:

- pst-server を PCP ノードにする実装コストが極めて大きい
- 既存の pst-core::peercast は HTTP API のみ、PCP は別実装

→ 本 ADR では採用しない。

### C. 設定追加

新フィールド (server config の `[recording]` に追加):

```toml
[recording]
auto_poll_interval_sec = 30   # 0 = 自動録画機能を無効化 (既定: 30)
auto_stop_grace_sec = 60      # 60 秒間チャンネルが消えてたら stop
```

`enabled = false` のままなら自動録画も走らない (既定 OFF を維持)。

---

## 複数チャンネル視聴アーキテクチャの設計

### A. Desktop (Tauri + libmpv)

#### 候補 A1. **新ウィンドウを 1 配信ごとに開く** (本 ADR で採用)

- 1 つの libmpv エンジン = 1 つの Tauri ウィンドウ = 1 配信
- 既存の "メインウィンドウ" を「**1 番目の視聴ウィンドウ**」とみなす
- 視聴中に「新しいウィンドウで開く」アクションを足す
  - 右クリック → 「新しいウィンドウで開く」
  - YP テーブルから Shift+クリックで新規ウィンドウ
- 各ウィンドウは独立した PlayerEngine state を持つ
  - 現状 `tauri::State<'_, PlayerEngine>` でアプリ単位に 1 つ → ウィンドウ
    単位の State 管理に変える必要あり
  - Tauri 2 は `app.handle().manage(...)` でしか State を持てないので、
    `HashMap<window_label, PlayerEngine>` を 1 つ常駐させる形になる

##### State 設計

```rust
pub struct PlayerRegistry {
    engines: Mutex<HashMap<String, Arc<PlayerEngine>>>,
}
```

`player_*` 系コマンドは `window_label: String` を引数で受け取り、
レジストリから該当エンジンを取得して操作する。既存の `player_attach`
は既に window_label を受け取っているので大きな手戻りは無い。

##### BBS ペインの扱い

- 各視聴ウィンドウは独自の BBS ペインを持つ (= チャンネルごとに別)
- 書き込み欄も独立

##### メリット / デメリット

- ✅ OS 標準のウィンドウ管理 (タイル / 重ね / マルチモニタ) が使える
- ✅ 既存コードのリファクタが比較的小さい (Tauri ウィンドウを増やすだけ)
- ❌ 4 つ並べると 4 ウィンドウになりタスクバーがごちゃつく
- ❌ libmpv x N で CPU / メモリ消費がかさむ

#### 候補 A2. 1 ウィンドウ内タイル表示 (見送り)

libmpv を複数インスタンス、それぞれ別の `wid` (ネイティブハンドル)
に attach。1 つの Svelte ページ内に `<div class="tile">` を 2x2 等で
並べ、各タイルの DOM ハンドルを wid として渡す。

技術的に可能だが:

- libmpv のウィンドウ埋め込みは OS ごとに差があり、複数 wid 管理は
  検証コストが高い
- Wayland では `wid` 経由埋め込み自体が不安定
- Svelte レイアウトと libmpv 描画 surface の同期が複雑

→ 採用しない。シンプル化を優先して A1 (ウィンドウ複数) を採る。

### B. pst-server Web フロント

#### 候補 B1. **グリッドビュー** (本 ADR で採用)

- ユーザーが「グリッドモード ON」にすると、`<video>` を N 個並べて
  HLS を同時再生
- 列数は CSS グリッドで `repeat(auto-fill, minmax(320px, 1fr))`
- 各タイルから「録画」「お気に入り追加」「閉じる」「単独表示」
- BBS は (とりあえず) 表示しない or 別タブ
- 上限: Web 側で 4-9 本まで (CPU / 帯域はクライアント裁量)

##### 実装

- 既存の `app.js` 構造を保ちつつ、`#player-section` を `#grid-section`
  と切り替え可能に
- `openChannel(id, name)` を「現在のセル」に追加する形に
- 各 tile は独立した hls.js インスタンスを持つ
- HLS プロキシ (`/hls/:id`) 側はそのまま (複数同時アクセスでも問題なし)

#### 候補 B2. PiP (Picture-in-Picture)

ブラウザ標準 PiP は 1 動画のみ。複数視聴には不向き。
→ 補助手段として将来追加検討。

### C. お気に入りからのまとめ起動

- お気に入りリストから「マッチした全部をまとめて開く」を 1 ボタンで
- Desktop: 新ウィンドウ x N を一括 spawn
- Server: グリッドに N 個一括追加

---

## 段階的な実装計画

各ステップを別 PR / commit にする。

### Step 1: 自動配信録画 (server only)

- pst-server に AutoRecorder task を追加
- config に `auto_poll_interval_sec` / `auto_stop_grace_sec`
- main.rs で `spawn(AutoRecorder::run(state))`
- 動作確認: お気に入りに `auto_record=true` を 1 つ入れ、
  PeerCastStation で該当配信を開始 → 自動で録画ディレクトリにファイル
  が出る

### Step 2: グリッド視聴 (Web)

- `/grid.html` or 同一 page 内モード切替
- N 本の `<video>` + hls.js
- お気に入り or YP リストから tile 追加 / 削除

### Step 3: Desktop ウィンドウ複数視聴

- PlayerRegistry 導入 (window_label → PlayerEngine)
- 既存 `player_*` コマンドに window_label を渡す
- 新コマンド `open_viewer_window(url)` で新ウィンドウ起動
- UI: 右クリック → 「新しいウィンドウで開く」

### Step 4: 自動視聴 (任意 / 後回し可)

- 「お気に入りの新規配信を自動でグリッドに追加」(Web)
- Desktop は 1 ウィンドウ 1 配信なので、「自動で新ウィンドウを spawn」
  は煩わしいので採用しない

---

## 関連 ADR / 既存実装

- ADR-0005: pst-server とワークスペース
- `pst-core::favorites`: ルール定義 + マッチング (92c2a9f)
- `pst-server::recording::RecordingState`: 並行録画タスク管理 (e0d3ccf,
  92c2a9f)
- `pst-server` Web UI 設定画面: 4d4d7f1

## TBD (実装着手前に決める)

- 自動録画の対象を「getChannels (紐付け先 PeerCast にいるもの)」だけに
  するか、「YP 取得結果 (peercast.yp_url) も含む」かをユーザーに確認
- 配信終了の判定方法 (channel が消えた = 1 回でも、grace = 60 秒で
  再出現を待つか) の閾値
- Desktop の複数ウィンドウ間で BBS Cookie / 設定 / ホットキーをどこ
  まで共有するか
- Web グリッドの帯域試算 (1 配信 1.5 Mbps × 9 = 13.5 Mbps、Wi-Fi 6 でも
  ギリギリ)
- マルチビュー時のステータスバー / 録画ボタンの取り扱い
