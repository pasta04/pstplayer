# 実機キャプチャ → リプレイによる高再現度の検証

スタブだけでは実機 (実 PeerCast / 実 BBS / 実 HLS) の挙動を完全には再現
できない。そこで **実機で本物のデータをキャプチャ**し、それを**リプレイ**する
ことで、ブラウザの無い環境 (Claude セッション等) でも実データに近い検証が
できるようにする。

ツールは `scripts/fixtures.py` (Python 標準ライブラリのみ、追加依存なし)。

## なぜ pst-server の REST 層で捕るのか

キャプチャ対象を **pst-server の REST 出力** (`/api/yp/all`, `/api/channels`,
`/api/channel/<id>/*`, `/api/board`, `/api/thread`, `/hls/...`) にしている。

- フロント (ブラウザ / ネイティブ) が実際に消費するのはこの REST 層なので、
  ここを実データで差し替えれば **UI をそのまま実データで検証**できる。
- BBS は URL の host で板種別を分類する都合上、上流 (したらば / 2ch) を
  ローカルで模倣するのが難しい。REST 層 (`/api/board`, `/api/thread`) の
  出力ごと捕れば、その問題を回避して**実 BBS のレスも丸ごと**保存できる。

## 手順

### 1. 実機 (ローカルセッション) でキャプチャ

実 PeerCast に向けた pst-server を起動しておく (どちらでも可):

```bash
pstplayer --server                 # 単一バイナリのサーバモード
# または
cargo run -p pst-server            # 単体サーバ
```

別ターミナルでキャプチャ (channel id / 板 URL / スレ URL は実環境のもの):

```bash
python scripts/fixtures.py capture \
    --server http://localhost:8080 \
    --channel <32hex-channel-id> \
    --board  "https://jbbs.shitaraba.net/bbs/subject.cgi/<cat>/<board>/" \
    --thread "https://jbbs.shitaraba.net/bbs/read.cgi/<cat>/<board>/<key>/" \
    --out fixtures
```

`fixtures/` に JSON / m3u8 / ts と `manifest.json` が保存される。

> **privacy**: fixture には実チャンネル名・IP・BBS 本文など機微情報が含まれ
> 得る。`fixtures/` は既定で `.gitignore` 済み。共有・コミットする場合は
> 中身を確認・サニタイズし、`git add -f` すること。

### 2. 任意環境 (Claude セッション等) でリプレイ

```bash
# フロントを併せて配信する場合は先に npm run build しておく
python scripts/fixtures.py replay --fixtures fixtures --web build --bind 127.0.0.1:8080
```

- ブラウザで `http://127.0.0.1:8080/hub` を開くと**実データ**で YP 一覧が出る。
- チャンネルを開けば `/watch` が実 HLS playlist / 実 BBS レスを (キャプチャ
  範囲で) 表示する。
- 書き込み (`POST /api/thread/post`) はリプレイでは受理のみの no-op。

`--web` を省略すると REST/HLS のみ配信 (curl 等での確認用)。

## 補足

- リプレイは pst-server の REST 互換 (CORS 全開) なので、`VITE_PST_API_BASE`
  でフロントの dev サーバから向けることもできる。
- HLS はセグメントを既定 3 本まで保存 (`--hls-segments` で増減)。実再生の
  目視はブラウザが要るが、playlist/セグメントの中継確認には十分。
- 取得に失敗したエンドポイント (例: スタブに JSON-RPC が無い等) は警告して
  スキップするので、部分キャプチャでも壊れない。
