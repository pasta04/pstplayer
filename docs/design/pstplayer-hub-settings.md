# PSTPlayer ハブ: 設定ダイアログ仕様

PeCaRecorder の「全般の設定」ダイアログ (PeerCast / YP / 通信 / プレイヤー
/ ツール / ブラウザ / ダウンロード / ブロックリスト / 通知 / シャットダウン
の各タブ) を参考に、pstplayer の機能体系に合わせて整理した設定仕様。

既存の Tauri 側 `設定ダイアログ` (`docs/ui-design.md §設定ダイアログ`
+ `src/routes/settings/`) を、ハブ画面の追加に合わせて拡張する。

## タブ構成 (PeCaRecorder 10 タブとの対応)

| PeCaRecorder のタブ | pstplayer での扱い | 備考 |
| --- | --- | --- |
| PeerCast | ✓ 採用 (一部) | host:port / basic auth / 自動起動 (低優先)。再生タブ連動は採用せず |
| YP | ✓ 採用 (拡張) | 複数 YP 登録 + 名前 / 名前空間 / URL / 表示色 / タブ表示有無 |
| 通信 | ✓ 採用 | タイムアウト / UserAgent / gzip。Proxy は低優先 |
| プレイヤー | ✗ 不採用 | libmpv 固定。外部プレイヤー登録 UI は持たない |
| ツール | ✗ 不採用 | 外部ツール登録は採用せず (前回確定済み) |
| ブラウザ | ✗ 不採用 | URL ブラウザは `tauri-plugin-opener` (OS デフォルト) で代替。BBS ブラウザは組み込み BBS ペインで代替 |
| ダウンロード (= 録画) | ✓ 採用 (一部) | サブフォルダ / リトライ / ファイル分割は採用。下記参照 |
| ブロックリスト | ✓ 採用 | お気に入りルールに `action = "ignore"` を新設して統合 |
| 通知 | ✓ 採用 | 既存 `tauri-plugin-notification` を活用、お気に入りマッチの新着のみ通知 |
| シャットダウン | ✓ 採用 (低優先) | 夜間自動録画運用向け。録画停止後に suspend / hibernate / shutdown |

pstplayer 固有で追加するタブ:

| タブ | 内容 |
| --- | --- |
| お気に入り | ルール編集 (条件 / 背景色 / 文字色 / pin_top / auto_record / ignore) |
| ハブ表示 | カラム ON/OFF / 幅 / 既定ソート / ダブル&ミドルクリック動作 |
| BBS | エンコード自動判定 / 投稿時 UA / Cookie 保持 (既存) |
| ショートカット | キーバインド一覧 (既存) |
| 履歴 | 最近開いたチャンネル (既存) |

## 各タブの詳細

### 接続 (PeerCast)

```toml
[peercast]
host = "localhost"        # 既存
port = 7144               # 既存
auth_user = ""            # 既存
auth_pass = ""            # 既存

# PeerCast 本体の自動起動 (ロードマップ「PeerCast 本体の自動起動・終了」)。
# 優先度: 低。実装は後回し。
[peercast.autostart]
enabled = false
exe_path = ""             # PeerCastStation.exe / peercast 等の絶対パス
args = []                 # 必要なら起動引数
stop_on_exit = false      # pstplayer 終了時に PeerCast も止めるか
```

PeCaRecorder の「再生タブ」関連設定は不採用 (pstplayer は視聴 = 別プロセス
ウィンドウなのでハブ画面の YP fetch とは独立、同期する必要がない)。

### YP

複数 YP 登録に対応 (現状の `peercast.yp_url` 単体から拡張):

```toml
[[yp.sources]]
name = "SP"
namespace = ""            # 名前空間 (任意、PeCaRecorder の名前空間と同義)
url = "http://bayonet.example/sp/"
show_tab = true           # ハブのタブバーにこの YP のタブを出すか
show_in_all = true        # 「すべて」タブに混ぜるか
text_color = "#000000"    # YP 由来のチャンネル行のデフォルト文字色
background = "#ffffff"    # YP 由来のチャンネル行のデフォルト背景色

[[yp.sources]]
name = "EP"
url = "http://eventyp.example/"
show_tab = true
show_in_all = true
```

ハブ画面のタブバーに YP ごとのタブが出る (新着 / 録画中 / 視聴中 の左に
追加)。お気に入りルールの色 + YP 由来の色は **お気に入り > YP 既定** の
優先度で適用。

優先度: 中。v1 はまず単一 YP (現状の `peercast.yp_url`) で動かしてから
拡張する。

### 通信

```toml
[net]
timeout_ms = 10000        # YP / PeerCast / BBS の HTTP タイムアウト
user_agent = ""           # 空なら "pstplayer/0.x.y" を自動使用
gzip = true               # Accept-Encoding: gzip をリクエストするか
relay_connect_timeout_ms = 1000  # PeerCast にリレー接続するときのタイムアウト

[net.proxy]
# LAN 利用想定では基本不要。WAN 越え / 検閲環境向け。優先度: 低。
enabled = false
url = ""                  # 例: "http://localhost:8080"
```

PeCaRecorder の `PeerCast Host:Port` フィールドは `[peercast]` 側 (上記)
と重複するので統合。

### プレイヤー (libmpv)

既存タブ。pstplayer 既存設定 (snapshot, recording, volume, aspect 等) を
そのまま使う。**ここに「外部プレイヤー登録」は追加しない**。

### お気に入り

ルール編集 UI を強化:

```toml
[[favorites.rules]]
name = "確実に録画するお気に入り"
channel_name = "inatami|nyamuru"   # 正規表現サポート
genre = ""
desc = ""
comment = ""
pin_top = true
auto_record = true
background = "#9be8df"             # 旧 color から改名
text_color = "#000000"             # 新規追加 (PeCaRecorder 相当)
```

**追加項目: `text_color`** (PeCaRecorder の「文字色」設定に対応)。既存の
`color` は `background` に rename (後方互換のため `color` も読める形で
deserialize する)。

サンプルプレビュー (PeCaRecorder と同じく現在の背景色 / 文字色で「サンプル」
ラベルを表示するボックス) も付ける。

### ハブ表示

```toml
[hub]
default_sort_column = "listeners"
default_sort_order = "desc"
double_click = "watch"             # "watch" | "record_start" | "open_bbs"
middle_click = "record_start"      # 同上

[hub.columns]
# カラム表示 ON/OFF と幅は pstplayer-hub-interactions.md 参照
```

### 録画 (PeCaRecorder の「ダウンロード」相当)

PeCaRecorder の Download タブから採用するもの:

```toml
[recording]
enabled = false                # 既存
dir = ""                       # 既存。出力先ディレクトリ
ext = ""                       # 既存。空なら flv
max_concurrent = 0             # 既存。0 = 8 本

# 新規追加 (PeCaRecorder 由来)
per_channel_subdir = false     # チャンネル名のサブフォルダを掘るか
filename_template = "{ts}_{name}"  # {ts}=YYYYMMDD_HHmmss / {name}=channel name / {id}=channel id
retry_count = 30               # 上流が落ちた時のリトライ最大回数
retry_interval_sec = 10        # リトライ間隔 (秒)
split_minutes = 0              # 0 = 分割なし、>0 = 指定分で次ファイルへ
split_megabytes = 1000         # 0 = 分割なし、>0 = 指定 MB で次ファイルへ
```

ファイル分割は長時間配信が 1 ファイル数 GB になるのを防ぐため。size /
time のいずれか先に達した側で分割。

**不採用**:

- 「指定リトライ数毎にチャンネル切断」「リトライ終了時チャンネル切断」 —
  pstplayer の録画は PeerCast に対する HTTP stream pull で完結するので、
  PeerCast 側のチャンネル切断は明示的に呼ばない (peercast.stop コマンドは
  別途明示操作で)

### ブロックリスト (= お気に入りに統合)

PeCaRecorder の Block list は独立した「除外フィルタ」UI だが、pstplayer
ではお気に入りルールに `action` を追加して一本化する:

```toml
[[favorites.rules]]
name = "NG"
channel_name = "麻雀|へたれ"
genre = ""
action = "ignore"              # "show" (既定) | "ignore" | "block"
                               #   show   = 普通に表示 (色 / pin_top / 録画は他フィールドで)
                               #   ignore = ハブから非表示 (削除はしない)
                               #   block  = 録画も視聴 spawn もしない
text_color = "#ffffff"         # 表示する場合の色 (ignore でも「表示」タブで見える)
background = "#e53935"
```

PeCaRecorder の「ブロックする操作」 (手動DL / 自動DL / 手動プレイヤー
/ 自動プレイヤー) の細かい粒度は採用せず、`ignore` / `block` の 2 段階で
簡素化。ホスト名ベースのブロックは Phase 2 (PeerCast から TIP の逆引きが
必要なので)。

### 通知 (Notifications)

既存 `tauri-plugin-notification` を使う:

```toml
[notification]
enabled = true                 # 通知機能を使うか
on_new_favorite = true         # お気に入りに当たる新着チャンネルが出現したとき
on_auto_record_start = true    # AutoRecorder が録画開始したとき
on_auto_record_stop = false    # 同停止 (うるさいので既定 OFF)
suppress_consecutive = true    # 同じイベントの連続抑制 (失敗が続いたら間引く)
```

音声は OS の通知音にまかせる (PeCaRecorder のように独自 wav 指定はしない、
優先度低)。

### シャットダウン (夜間運用向け)

```toml
[shutdown]
enabled = false                # 既定 OFF
mode = "none"                  # "none" | "suspend" | "hibernate" | "shutdown" | "app_only"

# 指定時刻に処理
schedule_hhmm = ""             # 例: "06:00"。空なら時刻トリガなし

# 録画していない状態が指定分続いたら処理
idle_after_minutes = 0         # 0 = 無効、>0 = 録画 0 本でこの分数経過

# 実行前にダイアログでキャンセル機会を与える
confirm_dialog_seconds = 300   # 0 = 即実行、>0 = 指定秒間ダイアログ表示
```

実装メモ: suspend / hibernate / shutdown は OS 別の syscall (Windows:
`SetSuspendState` / `ExitWindowsEx` / Linux: `systemctl suspend` / macOS:
`pmset sleepnow`) を呼ぶ。優先度低、実装は他のタブが落ち着いてから。

### BBS / ショートカット / 履歴

既存タブ。変更なし。

## マイグレーション

既存 `config.toml` から新スキーマへの移行は読み込み時に自動 (deserialize
時に `color` → `background`, 単一 `peercast.yp_url` → `[[yp.sources]]` 1
件、等)。書き戻し時は新スキーマで書く。

ユーザに見える挙動変化は無いはず (= 既存設定を読んでも UI は同じ)。
