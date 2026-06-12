# 実機 QA チェックリスト

リリース前 (とくに `0.1.0` 等の最初のメジャー / マイナータグを切る前)
に、3 OS の実機で踏むテスト項目。CI で検知できない以下を確認する目的:

- ネイティブ依存 (libmpv / WebView / GTK / AppKit) のリンク + ロード
- libmpv の `end_file.reason` の実値 (自動再接続の前提)
- ウィンドウ embedding (`wid` プロパティで libmpv が描画できるか)
- マルチプロセス / マルチウィンドウの体感品質
- 配布バイナリ (portable ZIP / app.zip / .deb / .AppImage / .rpm) の
  正常起動

各項目は「3 OS それぞれ」で確認。表に ✓/✗/N/A を埋めて Issue や
リリースノートに添付できる形式にする。

凡例: ✅ pass / ⚠ 動くが要観察 / ❌ fail / − 環境上テスト不能

---

## 0. 事前準備

- [ ] テスト対象タグを check out (例: `git checkout 0.1.0-rc.1`)
- [ ] GitHub Actions `Build artifacts` で生成された artifact を 3 OS 分
      ダウンロード ([release.md §6](release.md))
- [ ] テスト用の PeerCast 配信を 1 本以上立ち上げ (PeerCastStation
      推奨) + コンタクト URL に書き込み可能なテスト用 BBS を用意

---

## 1. インストール / 起動

| 項目 | Windows | macOS | Linux |
| --- | :---: | :---: | :---: |
| 配布物を解凍 / インストール | | | |
| アイコンが PSTPlayer 専用のもの (メガホン + 放射波) | | | |
| 初回起動 (引数なし) でハブ画面が出る | | | |
| 初回起動 (URL 引数あり) でビューア画面が出る | | | |
| `pstplayer --help` でヘルプが出る | | | |
| `pstplayer --version` でバージョンが出る | | | |
| プロセス終了でゾンビ残らない | | | |

**OS 別の特殊ケース**:
- Windows: SmartScreen 通過手順 (「詳細情報」→「実行」) が機能するか
- Windows: `libmpv-2.dll` を別フォルダに置くと起動失敗することを確認 (= 同梱必須)
- macOS: 初回 Cmd+クリック → 「開く」で Gatekeeper 通過
- macOS: `.app/Contents/Frameworks/libmpv.2.dylib` が同梱されているか確認
       (`otool -L PSTPlayer.app/Contents/MacOS/pstplayer` で `@rpath/libmpv.2.dylib` を参照)
- Linux: AppImage で `chmod +x` 必要 / `libfuse2` 未インストール時の挙動
- Linux: `.deb` で `apt` が `libmpv2` を自動解決
- Linux Wayland: 「動画が真っ黒で音だけ」になることを確認 → X11 で再試行

---

## 2. PeerCast 接続 / 設定

| 項目 | Windows | macOS | Linux |
| --- | :---: | :---: | :---: |
| 設定 → PeerCast でホスト / ポートを編集 → 保存 | | | |
| `localhost:7144` で接続成功 | | | |
| LAN 内別マシンの PeerCast に接続成功 (Basic 認証なし) | | | |
| Basic 認証付き PeerCast に接続成功 | | | |
| 起動時の接続失敗で設定ウィンドウが自動オープン | | | |
| `config.toml` が破損していると `.bak.YYYYMMDD_HHMMSS` 退避 + 既定起動 | | | |
| `recent_hosts` ドロップダウンが機能 | | | |

設定ファイル場所 (OS 別):
- Windows: `%APPDATA%\PSTPlayer\config.toml`
- macOS: `~/Library/Application Support/PSTPlayer/config.toml`
- Linux: `~/.config/PSTPlayer/config.toml`

---

## 3. ビューア画面 (URL 引数起動)

### 3.1 URL 受付バリエーション

| URL 形式 | 動作確認 |
| --- | :---: |
| `http://host:port/pls/{id}` | |
| `http://host:port/pls/{id}.flv` | |
| `http://host:port/stream/{id}.flv` | |
| `http://host:port/pls/{id}?tip=ip:port` | |
| `http://host:port/play.html?id={id}` | |
| 不正な URL → エラー表示 | |

### 3.2 CLI 引数

| 項目 | Windows | macOS | Linux |
| --- | :---: | :---: | :---: |
| `pstplayer "<url>"` で起動 → 自動再生 | | | |
| `pstplayer "<url>" "チャンネル名"` (PCRPlayer 互換) | | | |
| `--no-autoplay` で URL 受取のみ | | | |
| `--name=` / `--genre=` 等の長オプション反映 | | | |
| `--record-on-start` で起動と同時に録画開始 | | | |

### 3.3 再生制御

| 項目 | 確認 |
| --- | :---: |
| 動画が正常表示 (CSS overlay と zorder) | |
| 音声が出る | |
| ホイールで音量 0〜150% (5% 刻み) | |
| ミュート (`M`) | |
| ダブルクリックで全画面 (F11 同等) | |
| Esc で全画面解除 | |
| Ctrl+1〜9 でサイズプリセット | |
| Alt+1〜7 でアスペクト比 | |
| `T` 常に最前面 ON/OFF | |
| `Z` フレーム / `X` タイトル / `B` ステータスバー / `C` BBS ペイン | |
| `F5` Bump (再接続要求) | |
| `Ctrl+F5` Stop (PeerCast 切断) | |
| 視聴履歴メニューから過去チャンネル再生 | |

### 3.4 スナップショット

| 項目 | 確認 |
| --- | :---: |
| `F2` で保存先に PNG が生成 | |
| 1 秒以内連発で OS 通知が最後 1 件にまとめられる | |
| JPEG 設定で `.jpg` が生成 | |
| 保存先空欄で exe 配下 `snapshot/` に保存 | |
| AppImage / .app バンドル内で writable=false → fallback (Pictures) | |

---

## 4. BBS 読み書き

### 4.1 したらば JBBS

| 項目 | 確認 |
| --- | :---: |
| コンタクト URL = スレ URL で自動表示 | |
| コンタクト URL = 板 URL → `Ctrl+L` でスレ一覧 → 選択 | |
| 5 秒ごとの差分取得 (`If-Modified-Since`) が動く | |
| `Ctrl+R` 即時更新 / `Ctrl+Shift+R` 全レス再取得 | |
| `>>N` クリックでポップアップ | |
| ID クリックで同一 ID 抽出 | |
| URL の自動リンク化 + OS ブラウザで開く | |
| フィルタ欄: `(文字列)` / `>>N` / `>>N-M` / `id:xxx` | |
| 投稿: 名前 / メール / 本文 → Ctrl+Enter で確認 → 送信 | |
| 投稿成功で新着自動表示 | |
| 投稿規制時に「規制」エラー表示 | |
| 投稿失敗時に「拒否」エラー表示 | |
| スレ落ち時に 💀 表示 + 自動更新停止 | |

### 4.2 2ch 互換 (5ch / jpnkn 等)

上記 4.1 と同等項目 + 以下:

| 項目 | 確認 |
| --- | :---: |
| Shift_JIS 文字 (¥ 等) が正しく表示される | |
| Cookie 2 段階確認 (規制確認画面) の自動再送 | |
| dat 範囲取得 (`Range: bytes=N-`) が動く | |
| 304 / 416 ハンドリングで二重表示しない | |

### 4.3 表示モード

| 項目 | 確認 |
| --- | :---: |
| プレーンモード (既定) で表示 | |
| HTML モード切替 → タグ反映 + サニタイズ | |
| HTML モードで `<script>` / `javascript:` URL が除去される | |

---

## 5. ハブ画面 (引数なし起動)

### 5.1 起動 + YP

| 項目 | 確認 |
| --- | :---: |
| 引数なしで起動 → ハブ画面 (`/hub`) が出る | |
| 設定 → YP で複数 YP ソース登録 → 並行 fetch | |
| 同 `channel_id` の重複は上の YP が採用される | |
| YP fetch 失敗が expandable で表示される | |
| 60 秒間隔の自動 fetch が動く | |
| `F5` / `Ctrl+R` で手動 fetch | |
| フィルタ欄でリアルタイム絞り込み | |
| カラムソート (▲ ▼) が機能 | |

### 5.2 お気に入りハイライト

| 項目 | 確認 |
| --- | :---: |
| ルールマッチ行が背景色 / 文字色で強調表示 | |
| `pin_top` で最上位固定 | |
| `action=ignore` で行非表示 | |
| `action=block` で行完全除外 (= 自動録画も走らない) | |
| `\|` 区切り OR (`a\|b\|c`) | |
| 複数フィールド AND | |
| 新着お気に入りマッチで OS 通知 | |

### 5.3 タブ + クリック動作

| 項目 | 確認 |
| --- | :---: |
| すべて / お気に入り / 新着 / 視聴中 / 録画中 / YP 別タブ | |
| `Ctrl+1〜9` でタブ切替 | |
| `↑` / `↓` で行選択 + スクロール | |
| `Enter` で視聴 spawn | |
| `Shift+Enter` で視聴+録画 spawn | |
| ダブルクリックの動作が設定値通り | |
| ミドルクリックの動作が設定値通り | |
| 右クリックメニュー全項目 | |
| 「🌐 pst-server」ボタンで設定の URL を OS ブラウザで開く | |

### 5.4 別プロセス spawn + single_instance

| 項目 | 確認 |
| --- | :---: |
| 行クリックで `pstplayer.exe <url>` が別プロセスで起動 | |
| ハブを閉じても視聴ウィンドウは残る | |
| 視聴ウィンドウを閉じてもハブは残る | |
| 同 `channel_id` を 2 度開くと既存ウィンドウにフォーカス | |
| `「視聴中」` タブが 5 秒ごとに更新 | |
| 「✕ 全閉じ」で全視聴ウィンドウが close | |
| 視聴ウィンドウのクラッシュが他に波及しない | |
| stale lock ファイル (`$TMPDIR/pstplayer-locks/`) が自動掃除される | |

---

## 6. 録画

### 6.1 手動録画 (Desktop ビューア)

| 項目 | 確認 |
| --- | :---: |
| 右クリック → ⏺ 録画開始 で開始 | |
| ファイル名が `YYYYMMDD_HHmmss_<channel>.<ext>` | |
| 拡張子設定 (flv / mkv) が反映される | |
| 録画中はメニューが「⏹ 録画停止」に変わる | |
| tooltip に出力中パスが表示 | |
| 右クリック → ⏹ 録画停止 で停止 | |
| 配信終了で自動停止 | |
| アプリ終了で自動停止 + ファイルが正常 close | |
| 保存先空欄で exe 配下 `recordings/` に保存 | |
| 同秒連続録画開始 → ファイル名に `_2` / `_3` が付く | |
| 録画ファイルを VLC / mpv で正常再生できる | |

### 6.2 自動録画 (ハブ → spawn + favorites)

| 項目 | 確認 |
| --- | :---: |
| `auto_record=true` ルールマッチで視聴開始時に録画も開始 | |
| `Shift+Enter` で auto_record 無しチャンネルも録画開始 | |
| 「⏺ 視聴 + 録画開始」メニュー項目で同上 | |
| ハブから「⏹ 録画停止」IPC が機能 | |
| 「録画中」タブが ▶ + ● バッジ表示 | |

### 6.3 自動配信録画 (pst-server)

下記 §8 と合わせて。

---

## 7. libmpv 自動再接続 (★ 最重要 — 観察モードで挙動把握)

新規実装のため、**まず観察モード (設定 OFF) で reason 値の実機挙動を
把握** してから有効化する手順。詳細は
[`src-tauri/src/player/engine.rs`](../src-tauri/src/player/engine.rs)。

### 7.1 観察モード (`auto_reconnect = false`、既定)

以下の各操作を行った時、ステータス帯に「配信終了/切断 (reason=...)」
がどの reason 値で表示されるかを記録する。**実機で 1 周走らないと
有効モードを安全に enable できないため必須**。

| シナリオ | 期待される reason (予想) | 実際 (要確認) |
| --- | --- | --- |
| 配信者が PeerCast 配信を正常終了 | `eof` | |
| 自分のマシンの Wi-Fi を切る | `error` | |
| 自分の PeerCast 本体を kill (`pkill peercast`) | `eof` or `error` | |
| LAN 内の PeerCast を持つ別マシンの電源を抜く | `error` | |
| 上流ピアが落ちる (シミュレーション困難、自然発生待ち) | `eof` or `error` | |
| ユーザがメニューから「■ 切断」 | `stop` (Skip 扱い) | |
| 別チャンネルへ切替 (`load` 上書き) | `stop` → 新 `start_file` | |

stderr にも `player end_file: reason=... auto_reconnect=false decision=Skip { reason: Disabled }`
形式で出ているので、`tail -f` でログ確認すると確実。

### 7.2 有効モード (`auto_reconnect = true`)

7.1 で reason 値が想定通りなら有効化して以下を確認:

| 項目 | 確認 |
| --- | :---: |
| 設定 → プレイヤー → 「自動再接続を有効化する」→ 保存 | |
| 設定保存直後 (load 不要) に有効になる | |
| Wi-Fi を切る → ステータス帯「自動再接続中… 試行 1/10 (1秒後)」 | |
| Wi-Fi 復帰前に再試行が 1, 2, 4, 8, 16 秒 のバックオフで進む | |
| Wi-Fi 復帰 → 再接続成功して再生継続 | |
| 配信終了 (配信者停止) → 即切断 3 回連続 → 「自動再接続を停止しました (配信終了の可能性)」 | |
| 5 分間ずっと失敗 → 「自動再接続を停止しました (合計時間超過)」 | |
| 10 試行で復帰しない → 「自動再接続を停止しました (上限到達)」 | |
| 再接続待機中にユーザが「■ 切断」→ 再接続キャンセル | |
| 再接続待機中にユーザが**同じ URL を手動 reload** → 待機中の再接続はキャンセルされ二重 reload しない | |
| 再接続中にユーザが別チャンネル load → 新セッションでカウンタリセット | |
| **録画中に自動再接続が発火** → libmpv の stream-record が継続するか / 停止するかを記録 (mpv 仕様が不明確なため実測必須。停止する場合はハブの ● バッジも消えることを確認) | |
| stderr に各 decision がログされる | |

### 7.3 設定 OFF への切替

| 項目 | 確認 |
| --- | :---: |
| 再接続中に設定 → OFF にすると、待機中の reload はキャンセルされる | |

---

## 8. pst-server (常駐 + Web ビューア)

### 8.1 起動

| 項目 | Windows | macOS | Linux |
| --- | :---: | :---: | :---: |
| `pst-server` バイナリ単独起動 | | | |
| `--config <path>` で設定ファイル指定 | | | |
| `pst-server.toml` が無い時の動作 (空起動 + デフォルト出力) | | | |
| systemd ユーザ unit / launchd / Windows スタートアップで常駐起動 | | | |

### 8.2 API (`curl` で確認)

```bash
curl http://localhost:8080/api/channels             # → JSON channels
curl http://localhost:8080/api/yp                   # → JSON YP entries
curl -I http://localhost:8080/hls/{id}.m3u8         # → 200 + Content-Type application/vnd.apple.mpegurl
curl http://localhost:8080/api/favorites            # → JSON rules (read-only)
```

| 項目 | 確認 |
| --- | :---: |
| `/api/channels` が PeerCast の getChannels を中継 | |
| `/api/thread?url=...` で BBS スレ取得 | |
| `POST /api/thread/post` で BBS 書き込み | |
| `/hls/{id}.m3u8` プレイリストが上流 PeerCastStation から取得できる | |
| `/hls/{id}/{seg}.ts` セグメントが透過 | |
| Range / If-Modified-Since が上流に転送される | |
| `..` / `/` を含む segment 名で 400 返却 (パストラバーサル対策) | |
| `file://` 等の不正スキームを `/api/yp?url=...` で送ると 400 | |

### 8.3 Web UI (PWA)

| 項目 | iOS Safari | Android Chrome | Desktop ブラウザ |
| --- | :---: | :---: | :---: |
| `/` を開いてチャンネル一覧表示 | | | |
| 「ホーム画面に追加」(PWA インストール) | | | |
| Service Worker でオフライン時にも UI が出る | | | |
| HLS 再生 (Safari ネイティブ / hls.js) | | | |
| グリッドモード切替 → 2-5 列タイル | | | |
| iOS の「同時 unmute 不可」制約で 1 本のみ unmute | | | |
| 行クリック → タイル追加 (重複不可) | | | |
| `✕` で削除、`⏺` で録画 | | | |
| お気に入りハイライト + ★ マーク | | | |
| BBS 読み書きが動く | | | |
| `/settings.html` で設定編集 + 保存 | | | |

### 8.4 自動配信録画 (AutoRecorder task)

| 項目 | 確認 |
| --- | :---: |
| `recording.enabled=true, auto_record=true` ルール有りで polling 開始 | |
| `auto_poll_interval_sec` (既定 60) 間隔で getChannels | |
| 新規出現したマッチを自動 start | |
| `getChannels` から消えて `auto_stop_grace_sec` (既定 30) 経過で自動 stop | |
| 短い瞬断ではファイルが分割されない (grace 内で復活) | |
| `max_concurrent` を超える時は新規 start が 429 | |
| `enabled=false` / auto_record ルール無し時は polling 自体走らない | |
| 録画停止時に stop_flag → 1 秒猶予 → 正常 flush | |

### 8.5 SD カード保護

| 項目 | 確認 |
| --- | :---: |
| 既定で pst-server ログがディスクに書かれない | |
| `[log] debug=true, dir="..."` でのみ tracing-appender が日次ローテ | |
| HLS プロキシがディスクに書き込まないこと (`strace -e openat` で確認) | |

---

## 9. リリースワークフロー

タグ push → CI → GitHub Releases の流れ。**通常は `0.1.0-rc.1` 等で
ドライランしてから正式タグを切る**。

| 項目 | 確認 |
| --- | :---: |
| `git tag -a 0.1.0-rc.1 -m "RC1"; git push origin 0.1.0-rc.1` | |
| `build.yml` の build job が 3 OS で走る | |
| build 完了後 release job が動く (`refs/tags/0.1.0-rc.1`) | |
| 配布物が `pstplayer-0.1.0-rc.1-{os}-{arch}.{ext}` 形式 | |
| `SHA256SUMS.txt` が正しいハッシュを記録 | |
| `THIRD-PARTY-0.1.0-rc.1.md` が同梱 | |
| Release が prerelease 扱いになる (rc / beta / alpha) | |
| Release タイトルが `0.1.0-rc.1` (タグそのまま、PSTPlayer prefix 無し) | |
| Release notes が `.github/release.yml` のカテゴリ (🚀 / 🐛 / 📝 / 🔧) で整形 | |
| 前回タグ以降の PR が箇条書きで列挙される | |
| dependabot / skip-changelog ラベル PR は除外される | |
| 正式タグ `0.1.0` を切ると prerelease 扱いにならない | |

---

## 10. 長時間安定性 (= 1-4 時間)

| 項目 | 確認 |
| --- | :---: |
| 1 時間連続視聴 → メモリ増加が緩やか (リーク無し) | |
| 1 時間連続録画 → ファイルが破損せず再生可 | |
| ハブを 4 時間開きっぱなし → YP 自動更新が破綻しない | |
| pst-server を Pi で 24 時間連続稼働 → AutoRecorder が動き続ける | |
| ハブから 3-5 viewer を spawn → 30 分連続 → CPU / メモリが頭打ち | |
| 自動再接続が複数回発火しても異常なし (= state 機械が壊れない) | |

---

## 11. 既知の制約 (テスト不要 / N/A 項目)

以下は実装上の制約であり、テスト時に「failing」とマークしない:

- Linux Wayland: libmpv の `wid` embedding が使えない (X11 必須)
- pst-server: 認証なし (LAN 限定運用前提)
- 録画ファイル: MP4 等 moov 終端形式で途中切断時の破損
- スナップショット: 同秒連続でファイル名衝突を許容 (録画は uniqueness counter あり)
- 投稿 POST: 自動 retry なし (二重投稿リスク回避のため手動再送)

---

## 12. テスト結果の報告

QA 完了後、以下を含む報告を残す:

- このチェックリストを fill した markdown
- 各 OS のスクショ (起動画面 / 視聴中 / ハブ / 設定)
- libmpv reason 値の実測表 (§7.1)
- パフォーマンス計測 (RSS / CPU / 安定動作時間)
- 既知の問題があれば GitHub Issues に登録

報告のテンプレ:

```markdown
# QA Report: 0.1.0-rc.1 (YYYY-MM-DD)

Tested by: @your-handle
Environments:
- Windows 11 23H2 / x64
- macOS 14.5 / arm64 (M1)
- Ubuntu 24.04 LTS / x64 (X11)

## Summary
- pass: NN / NN
- warning: NN
- fail: NN

## libmpv end_file.reason 実測
(§7.1 表を埋めた結果をここにコピー)

## Open issues
- #N: ...
```

---

## 関連

- [`release.md`](release.md) — リリース手順 / 配布物命名
- [`features.md`](features.md) — 機能仕様
- [`usecases.md`](usecases.md) — ユースケース表
- [`usage/`](usage/) — ユーザマニュアル
