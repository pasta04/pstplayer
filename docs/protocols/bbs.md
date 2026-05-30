# BBS プロトコル参考メモ

PSTPlayer が対応する掲示板タイプとプロトコル仕様まとめ。読み取り (subject.txt / dat) と書き込み (write.cgi / bbs.cgi) を扱う。

**スコープ**: PeerCast コンタクト URL で多く使われる **したらば JBBS** と **2ch 互換 (5ch 等)** を対象。それ以外 (まちBBS、ぜろちゃんねる等) は v1.0 以降。

**参考元** (公開ドキュメント・実装):

- **読み取り**
  - [unacast — Read5ch.ts](https://github.com/pasta04/unacast/blob/master/src/main/readBBS/Read5ch.ts) (5ch 互換)
  - [unacast — ReadSitaraba.ts](https://github.com/pasta04/unacast/blob/master/src/main/readBBS/ReadSitaraba.ts) (したらば)
- **書き込み / 共通仕様**
  - [Monazilla 開発ドキュメント (5ch wiki)](https://info.5ch.net/index.php/Monazilla/develop)
  - [Monazilla/develop/dat (5ch wiki)](https://info.5ch.net/index.php/Monazilla/develop/dat)
  - [2ch 型掲示板まとめ wiki — bbs.cgi](https://scrapbox.io/2chtypebbs/bbs.cgi)
  - [2ch 型掲示板まとめ wiki — 仕様](https://scrapbox.io/2chtypebbs/%E4%BB%95%E6%A7%98)

---

## 1. 共通の考え方

| 観点         | したらば JBBS                      | 2ch 互換 (5ch 等)               |
| ------------ | ---------------------------------- | -------------------------------- |
| 文字コード   | **EUC-JP**                         | **Shift_JIS**                    |
| dat 区切り   | `<>` × 6 フィールド                | `<>` × 7 フィールド (末尾は空)   |
| dat の先頭   | レス番号フィールドあり             | レス番号フィールドなし (行番号)  |
| 投稿先 cgi   | `bbs/write.cgi/{board}/{key}/`     | `{host}/test/bbs.cgi`            |
| Cookie       | `NAME` / `MAIL` (省略可)           | 投稿確認のために必須 (PON 等)    |
| 特殊変換     | `&#65374;` → `～`                  | なし                             |

## 2. 共通インタフェース (Rust trait)

```rust
trait BoardClient: Send + Sync {
    /// 板の URL からスレッド一覧を取得 (subject.txt)
    async fn list_threads(&self, board_url: &Url) -> Result<Vec<ThreadSummary>>;

    /// スレッド本文を取得。差分取得用に前回状態を渡せる
    async fn fetch_thread(
        &self,
        thread_url: &Url,
        prev: Option<&FetchState>,
    ) -> Result<(Vec<Post>, FetchState)>;

    /// 投稿
    async fn post(&self, thread_url: &Url, req: &PostRequest) -> Result<PostOutcome>;
}
```

3 実装を用意: `ShitarabaClient`, `Ch2CompatClient`, (将来) `MachiBbsClient` 等。

URL の正規表現マッチで `dyn BoardClient` をディスパッチする `BbsRouter` を作る。

---

## 3. したらば JBBS

### 3.1 URL パターン

| 種別                   | URL 例                                                                             |
| ---------------------- | ---------------------------------------------------------------------------------- |
| 板トップ (HTML)        | `https://jbbs.shitaraba.net/{category}/{boardId}/`                                |
| スレ一覧               | `https://jbbs.shitaraba.net/{category}/{boardId}/subject.txt`                     |
| 板設定                 | `https://jbbs.shitaraba.net/bbs/api/setting.cgi/{category}/{boardId}/`            |
| スレ HTML              | `https://jbbs.shitaraba.net/bbs/read.cgi/{category}/{boardId}/{key}/`             |
| スレ dat (rawmode API) | `https://jbbs.shitaraba.net/bbs/rawmode.cgi/{category}/{boardId}/{key}/`          |
| 範囲指定               | 末尾に `{from}-{to}` (例: `100-200`)、または `{from}-` で続き全て                 |
| 投稿                   | `https://jbbs.shitaraba.net/bbs/write.cgi/{category}/{boardId}/{key}/`            |

### 3.2 subject.txt 形式

```
{datNumber}.cgi,{スレタイ} ({レス数})\n
```

- カンマ区切り (2ch 系と違う)
- 1 行 1 スレ

### 3.3 dat (rawmode.cgi) 形式

文字コード: **EUC-JP**。

1 行 1 レス、`<>` 区切りで **6 フィールド**:

| Index | フィールド         | 例                                |
| ----- | ------------------ | --------------------------------- |
| 0     | レス番号           | `1`                               |
| 1     | 名前 (HTML 可)     | `名無しさん`                      |
| 2     | メール             | `sage`                            |
| 3     | 日付 + ID          | `2026/05/30(土) 12:34:56 ID:xxxx` |
| 4     | 本文 (HTML 可)     | `これは&lt;test&gt;です`          |
| 5     | スレッドタイトル   | `テストスレ` (1 番目のみ正式)     |

### 3.4 投稿 (write.cgi)

```
POST https://jbbs.shitaraba.net/bbs/write.cgi/{category}/{boardId}/
Headers:
  Content-Type: application/x-www-form-urlencoded
  Referer:      https://jbbs.shitaraba.net/{category}/{boardId}/
  User-Agent:   Monazilla/1.00 PSTPlayer/{version}
  Cookie:       NAME=...; MAIL=sage     (必要に応じ)
Body (EUC-JP で URL encode):
  SUBJECT=<新規スレ時のみタイトル>
  NAME=<名前>
  MAIL=<メール>
  MESSAGE=<本文>
  DIR={category}
  BBS={boardId}
  KEY={key}            ← 既存スレへの返信時
  TIME=<UNIX 秒>
  submit=書き込む
```

### 3.5 特殊変換

- 受信側で `&#65374;` → `～` 変換 (したらば固有のエンティティ)
- HTML エンティティ (`&lt;` `&gt;` `&amp;` `&quot;` `&#NNNN;`) は復号

### 3.6 エラー判定

レスポンスは HTML。Body 中の以下キーワードで判定:

- 成功: `書きこみました` / `<!--RESULT::CHECK-->` 等
- エラー: `<!--RESULT::ERROR-->` + メッセージ
- 規制: `投稿できません` / IP/ホスト名表示

---

## 4. 2ch 互換 (5ch / .sc / jpnkn 等)

### 4.1 URL パターン

| 種別          | URL 例                                                       |
| ------------- | ------------------------------------------------------------ |
| 板トップ      | `https://{host}/{board}/`                                    |
| スレ一覧      | `https://{host}/{board}/subject.txt`                         |
| スレ HTML     | `https://{host}/test/read.cgi/{board}/{key}/`                |
| スレ dat      | `https://{host}/{board}/dat/{key}.dat`                       |
| 板設定        | `https://{host}/{board}/SETTING.TXT`                         |
| 投稿          | `https://{host}/test/bbs.cgi`                                |

### 4.2 read.cgi → dat への変換

ユーザが貼り付ける URL は `read.cgi` 形式が多い。dat に変換する正規表現:

```
^https?://(?<host>[^/]+)/test/read\.cgi/(?<board>[^/]+)/(?<key>\d+)/?
  ↓
https://{host}/{board}/dat/{key}.dat
```

### 4.3 subject.txt 形式

```
{datNumber}.dat<>{スレタイ} ({レス数})\n
```

- `<>` 区切り (したらばと違う)

### 4.4 dat 形式

文字コード: **Shift_JIS**。

1 行 1 レス、`<>` 区切りで **7 フィールド** (末尾 2 つは空のことが多い):

| Index | フィールド               | 例                                            |
| ----- | ------------------------ | --------------------------------------------- |
| 0     | 名前                     | `名無しさん`                                  |
| 1     | メール                   | `sage`                                        |
| 2     | 日付 + ID (+ BE)         | `2026/05/30(土) 12:34:56.78 ID:xxxxxxxx BE:..` |
| 3     | 本文 (改行は ` <br> `)   | `本文`                                        |
| 4     | スレッドタイトル         | (1 行目のみ正式)                              |
| 5     | (空または運営用)         | -                                             |
| 6     | (空または運営用)         | -                                             |

- レス番号は 1 始まりの **行番号** (フィールドにはない)
- ID は `日付` 部分の最後 ` ID:` 以降を切り出す
- 名前欄に `</b>...<b>` 形式でトリップ表示が混ざる

### 4.5 差分取得 (重要)

サーバ負荷軽減のため、**1 度取得した dat は差分のみ追加取得** するのが流儀:

```
GET https://{host}/{board}/dat/{key}.dat
  If-Modified-Since: <前回 Last-Modified>
  Range:             bytes={前回 content-length}-
  Accept-Encoding:   gzip, deflate
  User-Agent:        Monazilla/1.00 PSTPlayer/{version}
```

| ステータス | 意味                       | 対応                                          |
| ---------- | -------------------------- | --------------------------------------------- |
| 200        | 全体取得 (初回)            | 全体保存                                      |
| 206        | 部分応答 (差分)            | `content-range` で位置確認、末尾に連結        |
| 304        | 未更新                     | 何もしない                                    |
| 416        | Range 不正 (スレ短縮等)    | 状態リセット、初回扱いで再取得                |

状態管理:

```rust
struct FetchState {
    last_modified: Option<String>,
    last_byte: u64,       // 累積バイト数 (次回 Range 起点)
    last_count: u32,      // 累積レス数
}
```

### 4.6 投稿 (bbs.cgi)

```
POST https://{host}/test/bbs.cgi
Headers:
  Content-Type: application/x-www-form-urlencoded
  Referer:      https://{host}/{board}/
  User-Agent:   Monazilla/1.00 PSTPlayer/{version}
  Cookie:       PON=...; HAP=...    (初回は空、確認ページ後に発行)
Body (Shift_JIS で URL encode):
  bbs={board}
  key={key}              ← 返信時 (新規スレは subject)
  subject=<新規時のみ>
  FROM=<名前>
  mail=<メール>
  MESSAGE=<本文>
  time=<UNIX 秒>
  submit=書き込む
```

### 4.7 投稿フロー

5ch 系は 2 段階確認のことが多い:

1. 1 回目 POST → 確認ページ HTML (`<!-- 2ch_X:cookie -->` 等が含まれる)
2. Cookie (PON, HAP) を保存、必要なら追加パラメータを抽出
3. 同じ POST に Cookie を付けて再送 → 書き込み完了

### 4.8 エラー判定

レスポンス HTML の `<title>` および特殊コメントで判定:

| HTML 中のマーカ              | 意味                |
| ---------------------------- | ------------------- |
| `<!-- 2ch_X:true -->`        | 成功                |
| `<!-- 2ch_X:cookie -->`      | クッキー確認 (再送) |
| `<!-- 2ch_X:check -->`       | 確認画面            |
| `<!-- 2ch_X:error -->`       | エラー              |
| `<title>ＥＲＲＯＲ`          | エラー (旧式)       |

---

## 5. アンカー / ID 抽出 (共通)

両者共通のパース処理:

- **アンカー**: 本文中の `>>1`, `>>1-5`, `>>1,3,5` 等を検出してリンク化
- **ID 抽出**: 日付フィールドから `ID:[A-Za-z0-9+/]{8,}` を抜き出し、同一 ID のレスをハイライト可能に
- **URL リンク**: `https?://...` を検出して anchor 化 (HTML 出力モード時)
- **トリップ**: `名前◆xxxxxxxx` の `◆` 以降を別色で

## 6. HTML エスケープ / 復号 (共通)

dat から取得した本文は HTML エンティティでエスケープされている (`<br>` を除く):

- 復号対象: `&lt;` `&gt;` `&amp;` `&quot;` `&#NNNN;` `&#xHHHH;`
- 改行: `<br>` → `\n`

## 7. Cookie 永続化方針

2ch 互換系の Cookie (PON / HAP / FCM 等) と したらば の `NAME` / `MAIL` Cookie をどう保存するか:

### 方針

- **永続化する**: 起動のたびに投稿確認フローを最初からやり直すと UX が悪い (5ch 系は 2 段階確認あり)
- **対象**: BBS サーバから `Set-Cookie` で返された全 Cookie。ハードコード (PON/HAP のみ抜き取り等) しない
- **保存先**: OS 標準のデータディレクトリ配下
  - Windows: `%LOCALAPPDATA%\PSTPlayer\cookies\`
  - macOS: `~/Library/Application Support/PSTPlayer/cookies/`
  - Linux: `~/.local/share/PSTPlayer/cookies/`
- **ファイル分割**: ホストごとに 1 ファイル (`{host}.json`)。ホスト単位での削除を容易にする
- **フォーマット**: JSON 配列
  ```json
  [
    { "name": "PON", "value": "xxx", "expires": "2027-01-01T00:00:00Z",
      "secure": true, "http_only": true, "domain": ".5ch.net", "path": "/" }
  ]
  ```
- **パーミッション**: Unix 系では `0600` (所有者のみ読み書き)
- **暗号化**: しない (掲示板 Cookie は機密度低。OS のユーザ分離に依存)
- **期限切れの扱い**: 読み込み時に `expires` を見て破棄
- **UI からの削除**: 設定 → BBS → 「保存された Cookie をクリア」(全消去 / ホスト指定 / 期限切れのみ から選択)

### スコープ外 (永続化しない)

- 掲示板アカウントのログイン情報 (PSTPlayer はログイン機能を持たない)
- セッション ID 系で短期失効するもの (毎回再取得で問題ない)
- 投稿時の確認画面で出る一時 token (応答内のフォーム値、Cookie ではない)

### Rust 実装方針

- `reqwest` の `cookie_store` 機能を使い、ファイルベースの `CookieStore` を実装
- `cookie_store` crate (BSD-3-Clause) を採用検討。JSON シリアライズ対応あり
- メインプロセスから `Arc<Mutex<CookieJar>>` で共有 (全 BBS リクエストで同一 jar を再利用)

## 8. PSTPlayer 実装ファイル構成 (再掲)

```
bbs/
├── traits.rs        # BoardClient trait + 共通型
├── router.rs        # URL → 実装ディスパッチ
├── shitaraba.rs     # したらば実装
├── ch2.rs           # 2ch 互換実装
├── parse.rs         # dat パーサ (共通ロジック切り出し)
├── encoding.rs      # EUC-JP/Shift_JIS/UTF-8 + HTML エンティティ
├── anchor.rs        # アンカー / ID / URL 抽出
└── types.rs         # ThreadSummary, Post, FetchState, PostRequest 等
```

## 9. 法的・運用上の注意

- **書き込みは必ずユーザ確認ダイアログを通す**: 誤投稿防止
- **User-Agent には PSTPlayer の名前を明記する**: BBS 運営からの規制対象になった時、巻き添えを避けるため
- **連投制限はクライアント側でも持つ**: 連続投稿は最低 1 秒、推奨 3 秒間隔
- **規制された IP のテストはしない**: 当然
- **dat ファイルの保存はユーザのローカルのみ**: 二次公開しない
- **Yahoo!カテゴリ廃止後のしたらばの位置づけ**: 現在は LIVEDOOR が運営。利用規約準拠

## 10. 既知の落とし穴

- **したらばの subject.txt はカンマ区切り、2ch は `<>` 区切り**: 同じ「subject.txt」だが互換ではない
- **dat の最終行が改行で終わっていない場合がある**: 行分割時の処理に注意
- **したらばの本文中の改行**: 本文中に直接 `\n` が入る (2ch は `<br>`)
- **5ch の dat レスポンスが gzip 圧縮されることが多い**: `Accept-Encoding: gzip` を送るなら復号必須
- **2ch 系の Cookie 名 (PON / HAP / FCM 等) は時期で変わる**: ハードコードせず Set-Cookie を素直に保存
- **`subject.txt` のレス数表示が `({n})` でなく `(n)` の鯖もある**: 正規表現を柔軟に
- **JBBS の `rawmode.cgi` レスポンスにはヘッダ行 (`Size: N`) が含まれる場合がある**: 仕様の差分要確認
