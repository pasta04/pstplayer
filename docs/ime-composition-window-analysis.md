# 視聴画面の書き込み欄で IME 変換ウィンドウが左上 (0,0) に出る問題の解析

対象: 視聴画面 (`src/routes/+page.svelte`) の書き込み欄 (`textarea`)。全角入力
(日本語 IME) の未確定文字列 / 変換候補が、テキスト入力欄の位置ではなく
ウィンドウの左上に描画されることがある。本書はコードを変更せず、事象の
発生経路を一次資料 (本リポジトリ、mpv、wry、tao のソース) から整理したもの。

## 1. 事象と既知の観測 (実機 QA の記録より)

- 発生するのは **Windows (WebView2)**。変換ウィンドウはスクリーンの左上では
  なく **ウィンドウ (WebView) の左上**に出る。
- 「一度投稿すると直る」「ステータスバー等の別要素を一度クリックすると直る」
  「textarea を blur→focus し直すと直る」。
- 「他アプリとの往復では起きにくい」が、**動画領域 (mpv) をクリックしてから
  書き込み欄に入力すると再発**する (単窓でも発生)。
- 変換中 (compositionstart 後) に blur→focus すると変換自体が消える
  (`524360e` の全フォーカス再アンカーで「変換できなくなる」副作用が出た理由)。

これまでの対策の履歴:

| commit    | 内容                                                                                                     | 結果                                             |
| --------- | -------------------------------------------------------------------------------------------------------- | ------------------------------------------------ |
| `0be0001` | 書き込み欄フォーカス時に blur→focus で再アンカー                                                         | 改善するが再発あり                               |
| `22eb791` | ウィンドウ blur 時に書き込み欄のフォーカスを外す                                                         | 発症条件の見立てが違い撤回 (`524360e`)           |
| `524360e` | ウィンドウ focus / 書き込み欄 focus 時に 80ms 遅延 blur→focus                                            | 変換中に blur が走り変換不能になる副作用で撤去   |
| `deb01ae` | 視聴プロセスごとに WebView2 の UDF を分離 (ブラウザプロセス分離)                                         | **多窓間 (ハブ↔視聴、視聴↔視聴) の発症は解消**   |
| `c167d31` | ネイティブフォーカス取得後 1.5 秒以内の書き込み欄 focus に限り 100ms 後に一度だけ再アンカー (変換中は不可) | 単窓 (mpv→textarea) でも再発 |
| `0b52f68` | §5 B-1 を実装。再アンカーの起点を textarea の focus イベントから**ネイティブフォーカス取得**(`player:click` / `onFocusChanged(true)` / window focus) に変更し、§3.3-1 の順序依存を解消 | **再発** (起点は直ったが再アンカー自体が無効だった → §8) |
| 本コミット | 再アンカーの blur→focus を**フレーム境界を跨ぐ**形に変更 (§8 で確定した真因)。あわせて実機調査ログ (既定 OFF) を追加。現行 | 実機確認待ち |

## 2. 一次資料から確定した事実

### 2.1 mpv の埋め込み子ウィンドウはキーボードフォーカスを取らない

mpv `video/out/w32_common.c` (master) より:

- `--wid` 指定時、mpv は `WS_CHILD | WS_VISIBLE` (+ `WS_EX_NOPARENTNOTIFY`) の
  子ウィンドウを作り、**直後に `EnableWindow(w32->window, 0)` で無効化**する。
  無効化された窓はマウス入力もキーボードフォーカスも受け取れない
  (マウスは親 = 本アプリの `PstplayerVideoSurface` 窓に届く)。
- mpv 内に `SetFocus` の呼び出しは無く、`WM_MOUSEACTIVATE` も処理しない。
  `WM_SETFOCUS` / `WM_KILLFOCUS` は自身のフラグ更新のみ。
- IME は `update_ime_enabled()` で `ImmAssociateContext(window, NULL)` により
  既定で切り離されている。

→ 「mpv の窓がキーボードフォーカス / IME コンテキストを奪う」経路は**存在しない**。

### 2.2 本アプリ側の wid 窓 (`src-tauri/src/player/embed.rs`) もフォーカスを取らない

- `PstplayerVideoSurface` は `WS_CHILD | WS_VISIBLE` の子窓で、WNDPROC は
  `WM_MOUSEWHEEL` / `WM_LBUTTONDBLCLK` / `WM_RBUTTONUP` / `WM_LBUTTONUP` だけを
  横取りして Tauri イベント (`player:*`) に変換し、それ以外 (**`WM_LBUTTONDOWN`、
  `WM_MOUSEACTIVATE` を含む**) は `DefWindowProcW` に流す。
- `DefWindowProc` は `WM_MOUSEACTIVATE` に `MA_ACTIVATE` を返す。つまり動画を
  クリックすると **トップレベル窓が活性化される**が、子窓自身は `SetFocus` を
  呼ばないので、**キーボードフォーカスは活性化されたトップレベル窓 (tao の
  HWND) に落ちる**。

### 2.3 wry はトップレベル窓の `WM_SETFOCUS` を横取りして WebView2 へフォーカスを移譲する

wry 0.55.1 `src/webview2/mod.rs` (`parent_subclass_proc`) より:

```rust
WM_SETFOCUS | WM_ENTERSIZEMOVE => {
  let _ = (*controller).MoveFocus(COREWEBVIEW2_MOVE_FOCUS_REASON_PROGRAMMATIC);
}
```

→ 2.2 でトップレベル窓に落ちたフォーカスは、直後に **プログラム的に**
WebView2 (Chromium の HWND) へ戻される。

### 2.4 tao の `set_focus()` は `SetForegroundWindow` のみ

tao 0.35.3 `platform_impl/windows/window.rs`: `set_focus()` は「可視・非最小化・
非前面」のときだけ `force_window_active` (`SetForegroundWindow`、失敗時は Alt
キー合成 (`SendInput`) 後に再試行) を呼ぶ。`SetFocus` は呼ばない。

フロントの `onPlayerClick()` は `isFocused()` が false のときだけ
`show()`/`setFocus()` を呼ぶが、`player:click` は `WM_LBUTTONUP` で発火するため
その時点では 2.2 の活性化が済んでおり、通常はここで `setFocus()` は走らない。

### 2.5 現行のフロント側対策 (`c167d31`) の動作条件

`src/routes/+page.svelte`:

- `markNativeFocus()` は `<svelte:window onfocus>` と Tauri イベント
  `player:click` で呼ばれ、時刻を記録する。
- `onWriteFocus()` は textarea の **`focus` イベント**で呼ばれ、
  `markNativeFocus` から **1.5 秒以内**なら **100ms 後**に、`composing`
  でなく activeElement が textarea のままなら blur→focus する。

## 3. 発生メカニズムの推定

### 3.1 「ネイティブフォーカスの往復経路」が 2 通りある

| 経路 | 操作                                              | Win32 レベルで起きること                                                                                                    |
| ---- | ------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| A    | BBS ペイン / 書き込み欄 / ヘッダを直接クリック    | Chromium 自身の HWND が `WM_MOUSEACTIVATE` を処理して活性化とフォーカス取得を一体で行う (通常の活性化)                       |
| B    | **動画領域をクリック** (窓が非活性のとき)         | 非 Chromium の子窓 (`PstplayerVideoSurface`) が `MA_ACTIVATE` → フォーカスは **トップレベル HWND** へ → wry が `MoveFocus` → Chromium HWND |

経路 B では、Chromium から見ると「同一スレッド内の別 HWND へフォーカスが抜け
(`WM_KILLFOCUS`)、直後にプログラム的に戻ってくる (`WM_SETFOCUS`)」というフォーカス
往復が **DOM の activeElement は textarea のまま**で起きる。他アプリとの往復
(経路 A に近い、活性化を伴う正規の往復) では起きにくいという観測と整合する。

### 3.2 Chromium (WebView2) の IME 位置決めがこの往復で取りこぼす (推定)

Chromium は Windows で TSF (`TSFTextStore`) を使い、IME からの `GetTextExt`
問い合わせに対して **レンダラから届いた選択範囲 / 変換文字列の矩形**
(selection bounds / composition character bounds) を返す。この矩形は

- レンダラ側は **前回送った値と同じなら再送しない** (差分送信)、
- ブラウザ側は **ネイティブフォーカス喪失時に TextInputClient を切り離し、
  状態を捨てる**

という性質があるため、経路 B の往復後は「ブラウザ側に矩形が無いのに、
レンダラ側は変化が無いので送らない」状態になり得る。この状態で変換を始めると
`GetTextExt` が空矩形 (または未レイアウト) を返し、IME は **対象 HWND の
クライアント原点 = ウィンドウ左上**に候補ウィンドウを置く。

この節は Chromium のソースを本セッションで直接参照できていないため**推定**
だが、観測事実とはすべて整合する:

- 投稿で直る: `disabled` でフォーカスが `.posts` へ移り、再 `focus()` で選択矩形が
  実際に変化して再送される。
- 別要素クリックで直る / blur→focus で直る: 同上 (フォーカス変化で矩形が再送)。
- 変換中の blur で変換が消える: 変換のキャンセルは仕様どおり (副作用の理由)。
- ブラウザプロセス分離 (`deb01ae`) で多窓間が直った: 同一ブラウザプロセス内の
  別ウィンドウ間で TextInputClient が切り替わる別経路の問題を潰した。
- 単窓でも動画クリック後に再発: 経路 B。

(2026-09-13 追補: この推定のうち Chromium 側の機構は §7 で一次ソースにより
裏取りした。反証できた変種と残る未知点は §7.4 を参照)

### 3.3 現行対策 (`c167d31`) が取りこぼす条件

1. **イベント順序**: 経路 B でページフォーカスが戻るとき、Blink は textarea の
   `focus` (page 型) と `window` の `focus` を同じフォーカス変更処理の中で
   発火し、Tauri の `player:click` はさらに後から非同期に届く。textarea の
   `focus` が `window` の `focus` より先に処理されると、`onWriteFocus()` の
   `Date.now() - nativeFocusAt > 1500` 判定で**再アンカーがスキップされ得る**
   (前回の `markNativeFocus` が 1.5 秒以内に無い場合)。
2. **100ms の遅延中に変換が始まる**: IME ON で即入力すると `composing` ガードで
   再アンカーしない (再アンカーすると変換が消えるため、これ自体は正しい)。
   その最初の変換が左上に出る。
3. **1.5 秒より後の書き込み欄クリック**: 動画クリック後 1.5 秒を過ぎてから
   書き込み欄をクリックした場合は再アンカーしない。この場合は DOM フォーカスが
   変わるので 3.2 の理屈では矩形が再送されるはずだが、実機で「mpv→textarea で
   再発」と観測されており、時間窓の妥当性は未検証。

## 4. 実機での検証手順 (次のステップ)

コードを直す前に、3.1〜3.3 を実機で確定させるためのログ取り:

1. **DOM 側**: `window` の `focus`/`blur`、textarea の `focus`/`blur`
   (`relatedTarget` 込み)、`compositionstart`、`player:click`、Tauri
   `onFocusChanged` を `performance.now()` 付きで記録し、左上表示が出た回と
   出なかった回の順序・間隔を比較する (3.3-1 の順序問題の有無が分かる)。
2. **ネイティブ側**: Spy++ 等でトップレベル HWND、`Chrome_WidgetWin_0/1`、
   `PstplayerVideoSurface`、mpv の窓への `WM_KILLFOCUS` / `WM_SETFOCUS` /
   `WM_MOUSEACTIVATE` / `WM_IME_STARTCOMPOSITION` の宛先と順序を記録する
   (経路 B の「トップレベル経由の往復」が実際に起きているかの確認)。
3. **再現条件の切り分け** (各 10 回程度):
   - (a) 他アプリ → 動画クリック → 即変換 / (b) 他アプリ → BBS ペインクリック → 変換
   - (c) 窓が活性のまま動画クリック → 変換 (活性化を伴わない場合に起きるか)
   - (d) 書き込み欄にフォーカスを残したまま vs body にフォーカスを逃がした状態
4. **TSF 経路の切り分け**: `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS` に
   `--disable-features=TSFImeSupport` を足して IMM32 経路に切り替え、症状が
   消えるかを見る (常用ではなく切り分け用。フラグ名は Chromium の版で変わり得る)。
5. **環境差**: WebView2 Runtime の版、IME (MS-IME 新/旧、Google 日本語入力、
   ATOK) ごとの再現率。

## 5. 対策候補 (優先度順の私見)

- **B-1 (フロント) — 実装済み (本コミット)**: 「textarea の `focus` イベント」
  待ちをやめ、`player:click` / `onFocusChanged(true)` / window focus を受けた
  時点で、activeElement が textarea かつ `composing` でなければ blur→focus
  する (3.3-1 の順序依存を解消。動画クリック直後に限定するので `524360e` の
  副作用は出ない)。これで直らなければ次は A (ネイティブ) へ進む。
- **B-2 (フロント)**: 動画クリック時に DOM フォーカスを body へ逃がし、書き込み欄
  への入力を必ず「新規フォーカス」にする (`22eb791` の発想を動画クリックに限定)。
  入力途中の文字は `writeBody` に残るので実害は小さい。
- **A (ネイティブ)**: `video_wndproc` で `WM_MOUSEACTIVATE` 後にフォーカスを
  トップレベル経由にせず、WebView2 の HWND (トップレベル直下の
  `Chrome_WidgetWin_0`) へ直接 `SetFocus` する、または `ICoreWebView2Controller::MoveFocus`
  を wry を介さず呼ぶ。経路 B を経路 A に近づける根本寄りの手だが、wry の
  サブクラスとの二重処理になるため要検証。
- **C (上流)**: 4 の結果が経路 B + TSF 起因で確定したら WebView2 / wry へ
  再現手順付きで報告する。

## 6. まとめ

- mpv も本アプリの wid 子窓もキーボードフォーカスを取らない。発症の引き金は
  「動画 (非 Chromium 子窓) クリックによる活性化でフォーカスが**トップレベル
  HWND に落ち、wry がプログラム的に WebView2 へ戻す**」という往復 (経路 B)
  である可能性が高い。
- この往復は DOM フォーカスを変えないため、Chromium の IME 位置情報
  (選択矩形) の再送が起きず、最初の変換で候補窓が WebView 左上に出る (推定)。
- 現行対策は textarea の `focus` イベントと 1.5 秒 / 100ms の時間窓に依存して
  おり、イベント順序と即時変換の 2 点で取りこぼす余地がある。
- 修正に入る前に 4 のログ取りで経路 B と順序問題を確定させ、B-1 → A の順で
  当てるのが安全。

## 7. 追補: 一次資料による裏取り (2026-09-13, リモートセッション)

§2 の引用検証と §3.2「推定」の裏取りを、wry 0.55.1 の raw ソースと
Chromium main (2026-09 時点) を取得して行った結果。行番号は取得時点のもの。

### 7.1 §2 の引用はすべて実ソースと一致 (+ 追加事実)

- wry `parent_subclass_proc` の
  `WM_SETFOCUS | WM_ENTERSIZEMOVE => MoveFocus(PROGRAMMATIC)` は
  `src/webview2/mod.rs` L1254-1257 に実在する。サブクラス対象は
  トップレベル (tao) HWND で、Tauri の通常ウィンドウが通る `new()` は
  `new_in_hwnd(..., is_child = false)` (L82-91) →
  `if !is_child { attach_parent_subclass(parent, controller) }` (L538-541)
  で必ず装着される。経路 B は pstplayer の全ウィンドウに存在する。
- 追加の事実: wry はトップレベルと WebView2 の間に自前のコンテナ子窓を
  作っており (`create_container_hwnd`)、その WNDPROC も `WM_SETFOCUS` で
  `SetFocus(最初の子窓)` する (L186-198、dioxus#2900 対策)。つまり
  プログラム的フォーカス転送は二層ある。`WebView::focus()` も
  `MoveFocus(PROGRAMMATIC)` (L1493-1500)。
- mpv `w32_common.c` の記述 (WS_CHILD + `EnableWindow(0)`、`SetFocus`
  呼び出し無し、`WM_MOUSEACTIVATE` 未処理、`ImmAssociateContext(window,
  NULL)`) も master で再確認した。

### 7.2 Chromium の composition 矩形パイプライン (ソースで確定)

候補ウィンドウの位置決めに使う矩形は、次の一方通行のパイプラインでしか
IME に届かない:

1. renderer: `WidgetBase::UpdateCompositionInfo()` は
   `monitor_composition_info_` が false なら**計算すらしない**
   (`widget_base.cc` L1448-1450)。計算しても前回と同値なら**再送しない**
   (`ShouldUpdateCompositionInfo`, L1538-1549)。送るときは
   `ImeCompositionRangeChanged(range, character_bounds)`。
2. `monitor_composition_info_` を立てるのは browser 側からの
   `RequestCompositionUpdates(immediate, monitor)` だけ (L1510-1517)。
   その browser 側の呼び元は **TextInputState 更新を受けたとき**の
   `RWHVA::OnUpdateTextInputStateCalled` → `RequestCompositionUpdates(
   false, state.type != NONE)` (`render_widget_host_view_aura.cc`
   L3500, L3596-3602)。**DOM フォーカスも text input type も変わらない
   経路 B の往復では、この再武装イベントは発生しない。**
3. browser 側キャッシュ `composition_range_info_map_` は
   `ImeCompositionRangeChanged` でのみ埋まり (`text_input_manager.cc`
   L421-447)、`Register` 時は空 (L462-467)。
4. IME の `GetTextExt` は composition 中は
   `RWHVA::GetCompositionCharacterBounds` (キャッシュ参照、
   `index >= character_bounds.size()` なら false — L1873-1890) を引き、
   false なら **`TS_E_NOLAYOUT`** を返す (`tsf_text_store.cc` L406-441)。
   composition が無いときだけ `GetCaretBounds()` に落ちる。
5. 矩形が届いたときだけ `TSFTextStore::SendOnLayoutChange` →
   `OnLayoutChange(TS_LC_CHANGE)` が飛び、IME が再問い合わせする
   (L1524-1535)。届かなければ再問い合わせの契機が無い。

つまり「**composition が活性なのに renderer から矩形が一度も届かない**」
状態が成立すると、`GetTextExt` は毎回 `TS_E_NOLAYOUT`、`OnLayoutChange` は
発火せず、**その変換の間ずっと**候補窓は誤位置に置かれ続ける。§1 の
「一過性のちらつきではなく変換全体が左上に出る」観測と一致する。

### 7.3 `TS_E_NOLAYOUT` → 左上は IME 側の文書化済みフォールバック

Firefox の TSF 実装が同じ問題を踏んで IME 別ハックを実装しており、
MS-IME / Google 日本語入力が `GetTextExt` の `TS_E_NOLAYOUT` に対して
候補ウィンドウを**画面またはウィンドウの左上に置く**ことが Mozilla の
バグトラッカーに明記されている:

- [bug 1609675](https://bugzilla.mozilla.org/show_bug.cgi?id=1609675) —
  MS-IME candidate window sometimes appears and flickers at top-left
  corner of display ([TSF][TS_E_NOLAYOUT])
- [bug 1061604](https://bugzilla.mozilla.org/show_bug.cgi?id=1061604) —
  Google 日本語入力向け NOLAYOUT ハック /
  [bug 970860](https://bugzilla.mozilla.org/show_bug.cgi?id=970860) /
  [bug 1081993](https://bugzilla.mozilla.org/show_bug.cgi?id=1081993)
  (候補窓が常にブラウザウィンドウ左上)

一方 Chromium main の `TSFTextStore::GetTextExt` に Mozilla 型の IME 別
NOLAYOUT 回避ハックは見当たらない (素直に `TS_E_NOLAYOUT` を返す)。
「Chromium で矩形が欠けると日本語 IME は左上に出す」ことの傍証になる。

### 7.4 反証できた変種と、残った未知点

- **反証**: 「ネイティブフォーカス往復で TSF の document focus が迷子に
  なる」変種は否定してよい。Chromium は `ITfThreadMgr::AssociateFocus(
  attached_window_handle_, document_manager)` で **HWND に文書を紐付けて
  おり** (`tsf_bridge.cc` L650-683、L295-305 のコメントも参照)、HWND が
  ネイティブフォーカスを取り直せば OS 側が document focus を自動復元する。
- ネイティブ blur/focus で走る `InputMethodWinTSF::OnBlur/OnFocus` が
  やるのは TSF イベントルータと key event dispatcher の付け外しだけ
  (`input_method_win_tsf.cc` L64-85)。`SetFocusedClient` (スレッド focus /
  AssociateFocus の更新) は **aura フォーカスが変わったときだけ**
  (`OnDidChangeFocusedClient`, L176-181)。
- なお `OnWillChangeFocusedClient → ConfirmCompositionText` (L167-171) は
  「本当のクライアント変更が変換中に起きると変換が確定される」実装で、
  `524360e` の副作用 (再アンカーで変換が消える) の根拠そのもの。
- **残る未知点はひとつ**: 経路 B の往復の間に WebView2 runtime が
  RWHVA / renderer へ blur・focus をどこまで伝搬させ、それが
  `monitor_composition_info_` と送信条件をどう倒すのか。ここだけは
  Chromium 読解では確定できず、§4 の実機ログ (+ §7.5) 待ち。

### 7.5 §4 への追加手段: `ime` トレースカテゴリで NOLAYOUT を直接観測できる

7.2 の各点には `TRACE_EVENT("ime", ...)` が仕込まれている:
`TSFTextStore::GetTextExt` (start,end / DIP rect / screen rect)、
`RWHVA::GetCompositionCharacterBounds` (comp_char_rect)、
`WidgetBase::UpdateCompositionInfo`。したがって

```
WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--trace-startup=ime --trace-startup-duration=60 --trace-startup-file=C:\temp\ime-trace.json
```

で起動して再現させ、perfetto (ui.perfetto.dev) で開けば、「`GetTextExt`
が rect を返したか / NOLAYOUT だったか」「comp_char_rect イベントが
そもそも出ているか (= 矩形が browser に届いているか)」を Spy++ より
直接的に判別できる (フラグ名・可否は WebView2 の版で要確認。startup
トレースが使えない場合は §4-4 の `--disable-features=TSFImeSupport`
切り分けを先に)。

### 7.6 対策候補への影響

§5 の優先順位は変更なし。**B-1 (player:click / onFocusChanged 起点の
即時 blur→focus) の根拠が強くなった**: blur→focus は DOM フォーカス変化
なので TextInputState 更新 → `RequestCompositionUpdates` 再武装 (7.2-2)
→ 次の変換から矩形が流れる、という復旧経路をちょうど踏み直す。既知の
「投稿すると直る / 別要素クリックで直る」も全て同じ経路の再武装として
説明できる。変換開始**前**に済ませる必要がある点も 7.4 の
`ConfirmCompositionText` から自明。

## 8. 真因の確定: 同期 blur→focus では再武装が起きない (2026-09-14)

`0b52f68` (B-1) で再アンカーの**起点**は直ったが実機で再発した。調べた
結果、起点ではなく**再アンカーの実装そのもの**が無効だったことが
Chromium のソースで確定した。これは `0be0001` / `c167d31` / `0b52f68` の
3 回すべてに共通する欠陥で、過去の対策が「改善するが再発」を繰り返した
理由でもある。

### 8.1 TextInputState はフレーム単位 + 差分のみ送信される

- 送信契機は **`WidgetBase::DidBeginMainFrame()` → `UpdateTextInputState()`**
  (`widget_base.cc` L704-710)。つまり `focus()` の呼び出しごとではなく
  **BeginMainFrame ごとに 1 回**評価される。
- しかも `UpdateTextInputStateInternal` には差分チェックがある
  (L1254 以降):

  ```cpp
  // Only sends text input params if they are changed or if the ime should be
  // shown.
  if (show_virtual_keyboard || reply_to_request ||
      text_input_type_ != new_type || text_input_mode_ != new_mode ||
      text_input_info_ != new_info || ...) {
  ```

→ 同期的に `el.blur(); el.focus();` すると、**フレーム境界を跨がない**ので
BeginMainFrame の時点では type も info も直前と完全に同一になる。差分が
無いので **TextInputState は 1 回も送られない**。§7.2-2 のとおり
`RequestCompositionUpdates(monitor=true)` は TextInputState 更新を受けた
browser 側でしか呼ばれないため、**矩形の再武装が起きない** =
`GetTextExt` は `TS_E_NOLAYOUT` を返し続け、候補窓は左上のまま。

### 8.2 「投稿すると直る」が成立していた理由

投稿フローは `disabled = true` → `await tick()` → `disabled = false` →
`focus()` と**複数フレームに跨る**。そのため

1. あるフレームで `TEXT_INPUT_TYPE_NONE` が送られ (browser 側で
   `RequestCompositionUpdates(monitor=false)`)、
2. 次のフレームで `TEXT_INPUT_TYPE_TEXT_AREA` が送られて
   `RequestCompositionUpdates(monitor=true)` で**再武装**される

という 2 回の実遷移が発生する。これが唯一確実に直る操作だった理由で、
「別要素をクリックして戻す」「他アプリへ行って戻る」も同じ理屈。

### 8.3 対策

再アンカーを **blur → フレーム境界を跨ぐ → focus** に変更した
(`nextFrame()`: `requestAnimationFrame` 2 回 + 50ms のタイマー保険)。
最小化・遮蔽で rAF が止まる環境でもフォーカスを落としたままにしない。
待機中にユーザーが別要素へフォーカスを移していた場合は奪い返さない。

### 8.4 それでも直らない場合の実機ログ

視聴画面に既定 OFF の調査ログを入れた (§4-1 の代替)。DevTools で

```js
localStorage.setItem('pst.imeDebug', '1'); // 解除は removeItem
```

を実行して再読み込みすると、`[ime <ms>] <event> active=<tag>` 形式で
window / Tauri / player:click の各フォーカス取得、書き込み欄の focus、
`compositionstart` / `compositionend`、再アンカーの blur / refocus /
skip / abort が時刻付きで出る。左上に出た回と出なかった回でこの並びを
比較すれば、「再アンカーが走ったか」「変換開始が再アンカーより先か」が
切り分けられる。
