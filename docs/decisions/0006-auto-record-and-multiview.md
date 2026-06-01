# ADR-0006: 自動配信録画 + 複数チャンネル視聴アーキテクチャ

- **ステータス**: 検討中 (2026-06-01) — 高優先度タスクの設計
- **決定者**: pasta04 (リポジトリオーナー)

## 関連ドキュメント

- 想定するユースケースと「やらないこと」: [`docs/usecases.md`](../usecases.md)
- 既存ロードマップ: [`docs/roadmap.md`](../roadmap.md)

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

## 採用する全体構成 (確定)

ユーザー判断の結果 (会話 2026-06-01):

> Desktop について、YP 表示・録画を司るアプリケーションと、視聴処理を
> 行うアプリケーションに分ける。Server の方はアプリケーションを分けて
> もいいが、一緒にできるとスマートでよい。

これを踏まえ、**Desktop も Server も「ハブ機能」と「ビューア機能」を
別バイナリに分ける**。ただし「ハブ」は新規開発せず、既存の **pst-server
をそのまま流用する**:

| ロール | 担当バイナリ | 中身 |
| --- | --- | --- |
| **ハブ (= 司令塔)** | `pst-server` (Rust + axum + Web UI) | YP / お気に入り編集 / 録画タスク / 自動配信録画 / 設定編集 / 履歴 |
| **ビューア (= 視聴クライアント)** | `pstplayer` (Desktop, Tauri + libmpv) / モバイルブラウザ | 動画再生 / BBS 読み書き / 手動録画 / 視聴系ホットキー |

具体的な運用:

| 想定環境 | ハブの動かし方 | ビューア |
| --- | --- | --- |
| Pi / NAS で常駐録画したい | Pi に `pst-server` を systemd 等で常駐 | PC から `pstplayer` で視聴 / モバイルから Web UI |
| Desktop 単機運用 | 同じ PC で `pst-server` を Windows サービス / Linux systemd ユーザ unit / macOS launchd で常駐 (ウィンドウなし) | 同じ PC で `pstplayer` で視聴 |
| ライト用途 (自動録画不要) | `pst-server` 立てない | `pstplayer` だけで URL ペースト視聴 |

メリット:

- ✅ Desktop でも Server でも **コードは 1 系統** (`pst-server`) で済む
  → 「Server を一緒にできるとスマート」要件に合致
- ✅ ビューア (`pstplayer`) は視聴専用なので大改造不要 (= ハブ機能 /
  YP / 自動録画ロジックを既存 Tauri 側に増やさなくて良い)
- ✅ 自動録画は `pst-server` の常駐で実現 → 「ウィンドウが増えるのは
  邪魔」要件に合致 (常駐は systemd 等のサービス、Windows サービスや
  Linux daemon は通常画面に出ない)
- ✅ 複数視聴は **`pstplayer` を複数起動するだけ** (ウィンドウを増やす)
  → 別途実装する必要が小さい

デメリットと対処:

- ❌ Desktop 単機で「ライトに視聴したいだけの人」も pst-server の存在を
  意識する必要がある
  → 「自動録画したい人だけ pst-server を立てる、しない人は今まで通り」
  という運用で OK
- ❌ Pi なし環境で pst-server を Windows サービスとしてインストール
  する手順が必要
  → docs/usage/server.md に Windows サービス / Linux systemd の設置
  例を追記する

### Server (現状の構造をそのまま役割分離)

- pst-server プロセス = ハブ相当 (YP / 録画 / 設定 / 自動配信録画)
- ブラウザ tab / Window = ビューア相当 (`<video>` + BBS ペイン)
- グリッドビューは「複数ビューアを 1 ページに並べる」表示モード
- 「ハブ専用 view」(YP 一覧 + 録画状態 + 設定 へのリンク) も既に
  index.html がほぼ該当している → そのまま強化する

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

Desktop ビューア (`pstplayer`) は視聴専用とする。複数チャンネル視聴は
**`pstplayer` を複数起動 = ウィンドウを増やす方式** で実現する。

#### 採用方針: ウィンドウ複数

- 1 つのチャンネル = 1 つの `pstplayer` ウィンドウ (= 別プロセス)
- 視聴中に「新しいウィンドウで開く」アクションで子プロセスを spawn
  - 右クリック → 「新しいウィンドウで開く」
  - お気に入り / YP (= pst-server Web) からのリンクを `pstplayer` に
    渡すと別ウィンドウとして開く (PCRPlayer 互換の CLI 引数を流用)
- 各ウィンドウは独立した PlayerEngine + BBS を持つ
- 同じチャンネルを 2 度開いた場合は既存ウィンドウにフォーカスする
  (single_instance 周りで実装)

メリット:

- ✅ 検証コスト低 (既存 `pstplayer` がそのまま N 個立つだけ)
- ✅ OS 標準のウィンドウ管理 (タイル / 重ね / マルチモニタ) が使える
- ✅ libmpv の `wid` 周りも既存のままで OK (1 プロセス 1 wid)

デメリット:

- ❌ ウィンドウ数だけタスクバーが占有される (タイル WM ならむしろ
  歓迎、Windows / macOS の Dock では数が多いと邪魔)
- ❌ メモリは「libmpv × N」だが、本質的に複数視聴は重い処理なので
  仕方ない

#### 「1 ウィンドウ内タイル表示」を採用しなかった理由

過去案 (旧 A1) として「1 つの `pstplayer` 内に 4 タイル」を検討した
が、以下から見送り:

- libmpv の `wid` × N 管理は OS 別の挙動差 (とくに Wayland) があり、
  検証コストが過大
- Svelte レイアウトと libmpv 描画 surface の同期 (リサイズ追従) が
  複雑
- ウィンドウ複数で得られる柔軟性 (マルチモニタ配置等) を犠牲にする
  価値が薄い

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

採用方針 (Desktop は `pst-server` (ハブ) + `pstplayer` (ビューア複数
プロセス)、Server は `pst-server` だけ) を踏まえると、実装は次の順:

### Step 1: pst-server に自動配信録画タスク — ✅ 完了

- [x] 既存の `RecordingState` を再利用 (HashMap で同時複数本録画)
- [x] 新しい AutoRecorder task (`crates/pst-server/src/auto_record.rs`)
      を `main.rs` で `tokio::spawn`
- [x] `auto_poll_interval_sec` (既定 60) 秒間隔で `getChannels` →
      `pst_core::favorites::first_match(rules, &ch.info)` →
      `auto_record = true` なら `RecordingState::start` を発火
- [x] 配信が消えたら `auto_stop_grace_sec` (既定 30) 秒の grace 経過後
      に `RecordingState::stop` で自動停止
- [x] config (`RecordingConfig`) に `auto_poll_interval_sec` /
      `auto_stop_grace_sec` を追加
- [x] `[recording] enabled = false` または `auto_record=true` ルール
      が 0 件のときは polling 自体走らせない (待機ループのみ。Pi の
      CPU を浪費しない)
- [x] 重複起動を防ぐため `RecordingState::is_recording` を追加 (毎
      周期 409 ログを吐かない)

これで Pi / Desktop 同居運用の両方で自動録画が動く状態。次は実機での
動作確認 + Step 2 (Web グリッド) へ進む。

### Step 2: Web グリッド視聴 (`pst-server` Web) — ✅ 完了

- [x] 同一ページ内モード切替 (`⊞ グリッド` / `📋 一覧` トグル)
- [x] N 本の `<video>` タイル + 独立 hls.js (Apple Safari はネイティブ)
- [x] グリッドモードではチャンネル一覧をピッカーに転用 (行タップで追加)
- [x] お気に入りマッチタイルは ★ マーク + favorites.color 枠線
- [x] タイル × ボタンで個別削除 / 全停止ボタン (タイルがある時のみ表示)
- [x] タイル ⏺ ボタンで個別録画開始 / 停止 (7 秒間隔で状態同期)
- [x] iOS Safari 制約に合わせ「全タイル muted autoplay、unmute は 1 本」
      実装 (タイル本体タップでフォーカス → unmute、他は mute)
- [x] レスポンシブ列数: 2 (~480px) / 3 (~900px) / 4 (~1400px) / 5 (1400px+)
- [x] BBS ペインはグリッドモードでは非表示。タイル→単独モード遷移で復活
- 設計図: [web-grid-mockup.svg](../design/web-grid-mockup.svg)

### Step 3: Desktop ビューアを「視聴専用」に整理

`pstplayer` (Tauri 側) から「ハブ機能」を縮小:

- 残す: 動画再生 / BBS / 書き込み / 手動録画 / URL ペースト / CLI 引数
  受け取り / 視聴ホットキー / 設定 (ローカルの視聴系のみ)
- 撤去 or 軽量化: YP ウィンドウ / お気に入り編集 / 視聴履歴 一覧 /
  自動録画ロジック → 「pst-server がある場合はそちらに任せる、無くて
  も最低限動く」設計に

具体的には:

- 設定ダイアログから「お気に入り」「履歴」「YP URL」タブを残しつつ、
  「ハブ機能は pst-server に同等以上の機能あり」と注記
- メイン UI に「pst-server に接続して YP を見る」ボタン (= ブラウザを
  pst-server のホストで起動)

### Step 4: 複数視聴 (`pstplayer` 複数プロセス起動)

採用方針: **YP ウィンドウは「ハブ」として常駐 + 視聴は別プロセス**。

- YP ウィンドウ (`/yp`) は行クリックしても**閉じずに表示し続ける** (現在
  の挙動を維持。多くのチャンネルを順次見たり、複数同時に見たりするとき
  に、毎回 YP を開き直さなくて良いように)
- 行をクリックした時は、現状の `emit('yp:selected', ...)` で main ウィン
  ドウを更新する挙動から、**新しい `pstplayer.exe` プロセスを URL 引数
  付きで spawn する挙動**へ変更
- 各視聴ウィンドウは独立した OS プロセス = 独立 libmpv = 独立 BBS。
  クラッシュ耐性が高い (1 ウィンドウが落ちても他に波及しない)
- 同一 `channel_id` で既に開いていればフォーカスのみ移す。**`single_instance`
  は「アプリ全体で 1 つ」ではなく「`channel_id` ごとに 1 つ」のセマン
  ティクスに拡張**する必要がある (現状の `pst_core::single_instance`
  はアプリ単独起動用なので作り直しに近い)
- 視聴中に右クリック → 「別ウィンドウで開く」を残す (YP 経由しない
  サブメニューからのスポーン)
- YP ウィンドウを閉じても視聴ウィンドウは生き続ける (別プロセスなので
  当然そうなる)。OS のタイル / 仮想デスクトップ / マルチモニタを使って
  自由に配置できる

実装メモ:

- spawn は `std::env::current_exe()` + `std::process::Command::new(...).arg(url).spawn()`
- `channel_id` 単位の single instance は OS 別のロックファイル / 名前付き
  Mutex (Windows) / abstract namespace socket (Linux) / launchd の named
  port (macOS) のどれかで実装。**今回は最も移植性の高いロックファイル
  方式** (`$TMPDIR/pstplayer-{channel_id}.lock`、PID 入り、stale 判定付き)
  を採用する
- 起動済みプロセスを「前面化」する IPC: ロックファイルに自分の
  webview window のラベル / hwnd を書いて、後発プロセスが読み取って
  Tauri の `WebviewWindow::setFocus` 相当を別プロセス経由で呼ぶか、
  ロックファイルの PID にプラットフォーム別の「ウィンドウ前面化」
  syscall を投げる
- 共有設定は `pst-core::config` がすでに OS 標準パス参照 + ファイル
  ベース永続化なので、複数プロセスから読まれても困らない (書き込み
  は YP プロセス側だけが行う運用にすれば衝突しない)
- お気に入り / 自動録画ロジックは `pst-server` (常駐サービス) 側に
  寄せる方針 (Step 3) なので、複数プロセスから自動録画が二重発火する
  心配はない

### Step 5: pst-server 同居運用のドキュメント

- `docs/usage/server.md` を強化:
  - Pi での systemd unit 例 (既出)
  - **Windows サービスとしての登録例** (sc create / nssm / WinSW)
  - **Linux systemd ユーザ unit 例**
  - **macOS launchd plist 例**
- 「自動録画したいだけのライト Desktop 利用者」が迷わず立てられる
  ように

### Step 6: 自動グリッド (任意 / 後回し可)

- pst-server Web で「お気に入り `auto_grid = true` のチャンネルが
  配信開始したらグリッドに自動追加」

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
