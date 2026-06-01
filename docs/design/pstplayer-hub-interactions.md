# PSTPlayer ハブ UI: ソート / 右クリックメニュー仕様

PeCaRecorder の挙動 (スクリーンショット添付) を参考に、pstplayer の機能
体系に合わせて整理したインタラクション仕様。レイアウトは
[pstplayer-hub-mockup.svg](pstplayer-hub-mockup.svg) (compact 1050) と
[pstplayer-hub-mockup-wide.svg](pstplayer-hub-mockup-wide.svg) (全カラム
1780) を参照。

## カラムソート

- すべてのカラムヘッダはクリックで昇順/降順を切替
- 初回クリック → 昇順 (▲)、再クリック → 降順 (▼)
- 別カラムクリック → そちらに切替 (前のソート方向はリセット)
- 既定: **👤 リスナー数 降順**
- `pin_top = true` のお気に入り行は **ソートよりも上位に固定** (現行の
  YP ペインと同じ振る舞いを継承)
- 文字列カラム (チャンネル名 / ジャンル等) は **日本語ロケール照合**
  (`compareLocale('ja')`) でソート

## 行操作 (デフォルト動作)

| 操作              | 既定動作                     | 設定可能 |
| ----------------- | ---------------------------- | -------- |
| 左クリック        | 行選択 (ハイライトのみ)      | ×        |
| ダブルクリック    | 視聴 (別ウィンドウで開く)    | ✓        |
| ミドルクリック    | 録画開始 (トグル)            | ✓        |
| 右クリック        | コンテキストメニュー         | ×        |

ダブルクリック / ミドルクリックの既定動作は `config.toml`:

```toml
[hub]
double_click = "watch"        # "watch" | "record_start" | "open_bbs"
middle_click = "record_start" # 同上
```

## 右クリックメニュー (PeCaRecorder の構造を踏襲、pstplayer の機能に合わせて整理)

```
▶ 視聴 (別ウィンドウで開く)              [Enter]    ← spawn_viewer (Step 4)
─────────────────────────────────────────────────
📺 BBS としてコンタクト URL を開く                  ← 既存 BBS ペインを新ウィンドウで
🌐 コンタクト URL をブラウザで開く                  ← tauri-plugin-opener
ℹ️ チャンネル情報を表示 (リレー数 / 上流 IP 等)     ← fetch_channel_status
─────────────────────────────────────────────────
⏺ 録画開始 / ⏹ 録画停止                  [Ctrl+R]   ← record_start / record_stop (toggle)
📂 録画ディレクトリを Explorer / Finder で開く      ← tauri-plugin-opener
─────────────────────────────────────────────────
📋 コピー                              ▶ (サブ)
★ お気に入りルールに追加              ▶ (サブ)
─────────────────────────────────────────────────
⚙ ダブルクリックの動作                ▶ (サブ)
⚙ ミドルクリックの動作                ▶ (サブ)
```

### 📋 コピー サブメニュー

```
チャンネル名
チャンネル詳細 (1 行: ジャンル / 詳細 / コメント)
チャンネル詳細 (複数行)
コンタクト URL
プレイリスト URL (= http://{peercast}/pls/{id})
ストリーム URL (= resolve_stream_url の結果)
channel ID (32 hex)
配信元 IP (TIP)
```

### ★ お気に入りルールに追加 サブメニュー

```
新規ルール作成…              ← ルール編集ダイアログ (channel_name 自動入力)
──────────────────────
NG
イベント告知
確実に録画するお気に入り
専スレお気に入り
本人お気に入り
(以下 favorites.rules の名前を全てリスト)
```

既存ルール選択時:

- 「このチャンネル名 (例: `inatami`) でマッチするように `channel_name`
  パターンに OR で足す」を確認ダイアログで提案
- ユーザは「OR で追加」「ルール条件を見直す (= 編集ダイアログを開く)」
  を選択可

### ⚙ ダブルクリックの動作 サブメニュー (チェック式ラジオ)

```
✓ 視聴 (別ウィンドウで開く)     ← 既定
  録画開始
  BBS を開く
```

選択結果は `config.hub.double_click` に永続化。ミドルクリックも同様。

### PeCaRecorder にあって pstplayer では実装しないもの

- **ツール → ツール設定**: pstplayer は組み込みプレイヤー (libmpv) 固定
  なので「外部プレイヤー選択」UI は不要
- **チャット URL を開く**: PeerCast のチャット (IRC 系) 運用はほぼ廃れて
  いるので非対応
- **クリップボードへコピー > IP アドレス / ホスト名**: TIP コピーで代替

## カラムの表示制御

- **ヘッダ右クリック** → カラム一覧のチェックボックスで ON / OFF
- **ヘッダ境界ドラッグ** → カラム幅変更
- 設定は `config.toml` の `[hub.columns]` に永続化:

  ```toml
  [hub.columns]
  channel_name   = { visible = true,  width = 160 }
  description    = { visible = true,  width = 460 }
  listeners      = { visible = true,  width = 60  }
  bitrate        = { visible = true,  width = 50  }
  uptime         = { visible = true,  width = 50  }
  type           = { visible = true,  width = 40  }
  status         = { visible = false, width = 50  }
  filter         = { visible = true,  width = 130 }
  recording      = { visible = true,  width = 60  }
  contact_url    = { visible = true,  width = 220 }
  yp             = { visible = false, width = 40  }
  yp_url         = { visible = false, width = 140 }
  channel_id     = { visible = false, width = 200 }
  tip            = { visible = false, width = 100 }
  ```

- 既定 ON: チャンネル名 / 詳細 / 👤 / kbps / 配信時間 / 形式 / フィルタ /
  録画 / コンタクト
- 既定 OFF (上級者向け): ステータス / YP / YP URL / channel ID / TIP

## タブ (上部)

| タブ        | 表示条件                                                   |
| ----------- | ---------------------------------------------------------- |
| すべて      | 取得した全チャンネル                                       |
| お気に入り  | `favorites.rules` のいずれかにマッチした行                 |
| ● 録画中    | 現在録画中 (`/api/record/list` または local recording 状態) |
| 視聴中      | 自プロセスから spawn 済みの視聴ウィンドウがある channel     |
| 新着        | 前回 fetch には無く今回 fetch で初めて出てきた channel       |

タブのカウントは括弧で表示 (例: `お気に入り (8)`)。

## キーボードショートカット (主要)

| キー          | 動作                                |
| ------------- | ----------------------------------- |
| `F5` / `Ctrl+R` | 更新 (YP 再取得)                  |
| `↑` / `↓`     | 行移動                              |
| `Enter`       | 視聴 (= ダブルクリック相当)         |
| `Ctrl+Shift+R`| 録画開始 / 停止 (選択行)           |
| `Ctrl+F`      | フィルタ入力欄にフォーカス          |
| `Ctrl+,`      | 設定ダイアログを開く                |
