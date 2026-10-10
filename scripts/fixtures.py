#!/usr/bin/env python3
"""PSTPlayer 検証用フィクスチャ ツール (capture / replay)。

実機 (ローカルセッション) で実 PeerCast に向けた `pstplayer --server` を立て、
その REST/HLS 出力を `capture` で保存する。保存した fixture を `replay` が
同じパスで配信するので、ブラウザ無し環境 (Claude 等) でも「実データ」を使った
高再現度の検証ができる。捕捉層を pst-server の REST にしているので、BBS
(host 依存の分類で上流模倣が難しい) も /api/board, /api/thread の出力ごと
捕れるのが利点。

依存なし (標準ライブラリの urllib / http.server のみ)。

使い方:
  # 実機 (ローカル, 実 PeerCast に向けた pst-server が :8080 で稼働中):
  python scripts/fixtures.py capture --server http://localhost:8080 \\
      --channel <32hex> --board <BBS板URL> --thread <スレURL> --out fixtures

  # 任意環境 (Claude 等) で fixture を配信:
  python scripts/fixtures.py replay --fixtures fixtures --web build --bind 127.0.0.1:8080
  #   → ブラウザで http://127.0.0.1:8080/hub を開けば実データで動く

注意: fixture には実チャンネル名/IP/BBS 本文など機微情報が含まれ得る。
公開リポジトリにコミットする前に必ず中身を確認・サニタイズすること
(既定では fixtures/ は .gitignore 済み)。
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import urllib.parse
import urllib.request
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

MANIFEST = "manifest.json"


# ── capture ──────────────────────────────────────────────────────────────
def _get(url: str) -> tuple[int, bytes, str]:
    req = urllib.request.Request(url, method="GET")
    try:
        with urllib.request.urlopen(req, timeout=30) as r:
            return r.status, r.read(), r.headers.get("Content-Type", "")
    except urllib.error.HTTPError as e:
        return e.code, e.read(), e.headers.get("Content-Type", "")


def _save(path: str, data: bytes) -> None:
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "wb") as f:
        f.write(data)
    print(f"  saved {path} ({len(data)} B)")


def capture(args: argparse.Namespace) -> int:
    base = args.server.rstrip("/")
    out = args.out
    manifest: dict = {"server": base, "channels": [], "board": {}, "thread": {}, "hls": []}

    def grab(api_path: str, file_path: str) -> bytes | None:
        status, data, _ = _get(base + api_path)
        if status != 200:
            print(f"  WARN {api_path} -> HTTP {status} (skip)")
            return None
        _save(os.path.join(out, file_path), data)
        return data

    print("== capture: YP / channels / config ==")
    grab("/api/yp/all", "api/yp/all.json")
    grab("/api/channels", "api/channels.json")
    grab("/api/config", "api/config.json")
    grab("/api/record/list", "api/record/list.json")

    for cid in args.channel or []:
        print(f"== capture: channel {cid} ==")
        grab(f"/api/channel/{cid}/info", f"api/channel/{cid}/info.json")
        grab(f"/api/channel/{cid}/status", f"api/channel/{cid}/status.json")
        # HLS playlist + 先頭数セグメント。
        pl = grab(f"/hls/{cid}/index.m3u8", f"hls/{cid}/index.m3u8")
        if pl is not None:
            manifest["channels"].append(cid)
            manifest["hls"].append(cid)
            segs = [
                ln.strip()
                for ln in pl.decode("utf-8", "replace").splitlines()
                if ln.strip() and not ln.startswith("#")
            ]
            for seg in segs[: args.hls_segments]:
                # 相対セグメントのみ対応 (PeerCastStation の通常出力)。
                if "/" in seg or seg.startswith("http"):
                    continue
                grab(f"/hls/{cid}/{seg}", f"hls/{cid}/{seg}")

    for url in args.board or []:
        print(f"== capture: board {url} ==")
        q = urllib.parse.urlencode({"url": url})
        data = grab(f"/api/board?{q}", f"api/board/{_hash(url)}.json")
        if data is not None:
            manifest["board"][url] = f"api/board/{_hash(url)}.json"

    for url in args.thread or []:
        print(f"== capture: thread {url} ==")
        q = urllib.parse.urlencode({"url": url})
        data = grab(f"/api/thread?{q}", f"api/thread/{_hash(url)}.json")
        if data is not None:
            manifest["thread"][url] = f"api/thread/{_hash(url)}.json"

    _save(os.path.join(out, MANIFEST), json.dumps(manifest, ensure_ascii=False, indent=2).encode())
    print(f"\nDone. レビュー/サニタイズの上で利用してください: {out}/")
    return 0


def _hash(s: str) -> str:
    # ファイル名に使える短いキー (衝突は実用上無視できる範囲)。
    import hashlib

    return hashlib.sha1(s.encode("utf-8")).hexdigest()[:16]


# ── replay ───────────────────────────────────────────────────────────────
def make_handler(fixtures: str, web: str | None):
    manifest_path = os.path.join(fixtures, MANIFEST)
    manifest: dict = {}
    if os.path.exists(manifest_path):
        with open(manifest_path, encoding="utf-8") as f:
            manifest = json.load(f)

    def content_type(path: str) -> str:
        if path.endswith(".json"):
            return "application/json"
        if path.endswith(".m3u8"):
            return "application/vnd.apple.mpegurl"
        if path.endswith(".ts"):
            return "video/mp2t"
        if path.endswith(".html"):
            return "text/html"
        if path.endswith(".js"):
            return "text/javascript"
        if path.endswith(".css"):
            return "text/css"
        if path.endswith(".png"):
            return "image/png"
        return "application/octet-stream"

    class Handler(BaseHTTPRequestHandler):
        def _send_file(self, rel: str, ctype: str | None = None) -> bool:
            full = os.path.join(fixtures, rel)
            if not os.path.isfile(full):
                return False
            with open(full, "rb") as f:
                data = f.read()
            self._send(200, data, ctype or content_type(rel))
            return True

        def _send(self, status: int, data: bytes, ctype: str) -> None:
            self.send_response(status)
            self.send_header("Content-Type", ctype)
            self.send_header("Content-Length", str(len(data)))
            self.send_header("Access-Control-Allow-Origin", "*")
            self.end_headers()
            self.wfile.write(data)

        def _send_web(self, path: str) -> None:
            # SPA: 静的ファイルがあれば返し、無ければ index.html。
            if web:
                rel = path.lstrip("/") or "index.html"
                full = os.path.join(web, rel)
                if os.path.isfile(full):
                    with open(full, "rb") as f:
                        self._send(200, f.read(), content_type(full))
                    return
                idx = os.path.join(web, "index.html")
                if os.path.isfile(idx):
                    with open(idx, "rb") as f:
                        self._send(200, f.read(), "text/html")
                    return
            self._send(404, b"not found", "text/plain")

        def do_POST(self) -> None:
            # 書き込み系は replay では受理して no-op (UI の動作確認用)。
            length = int(self.headers.get("Content-Length", 0))
            if length:
                self.rfile.read(length)
            self._send(200, b"{}", "application/json")

        def do_GET(self) -> None:
            parsed = urllib.parse.urlparse(self.path)
            path = parsed.path
            qs = urllib.parse.parse_qs(parsed.query)

            # query で引く BBS 系。
            if path == "/api/board" and "url" in qs:
                rel = manifest.get("board", {}).get(qs["url"][0])
                if rel and self._send_file(rel):
                    return
                self._send(404, b'{"code":"not_captured","message":"board"}', "application/json")
                return
            if path == "/api/thread" and "url" in qs:
                rel = manifest.get("thread", {}).get(qs["url"][0])
                if rel and self._send_file(rel):
                    return
                self._send(404, b'{"code":"not_captured","message":"thread"}', "application/json")
                return

            # パスから直に引ける REST / HLS。
            if path.startswith("/api/") or path.startswith("/hls/"):
                rel = path.lstrip("/")
                if path.startswith("/api/") and not path.endswith(".json"):
                    rel = rel + ".json"
                if self._send_file(rel):
                    return
                if path == "/api/record/list":
                    self._send(200, b'{"recordings":[]}', "application/json")
                    return
                self._send(404, b'{"code":"not_captured","message":"' + path.encode() + b'"}', "application/json")
                return

            # それ以外は静的フロント (SPA)。
            self._send_web(path)

        def log_message(self, *a):  # 静かに。
            pass

    return Handler


def replay(args: argparse.Namespace) -> int:
    handler = make_handler(args.fixtures, args.web)
    host, _, port = args.bind.rpartition(":")
    server = ThreadingHTTPServer((host or "127.0.0.1", int(port)), handler)
    print(f"replay: http://{args.bind}  fixtures={args.fixtures}  web={args.web or '(none)'}")
    print("Ctrl-C で停止。")
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        pass
    return 0


# ── entry ────────────────────────────────────────────────────────────────
def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description="PSTPlayer fixture capture/replay")
    sub = p.add_subparsers(dest="cmd", required=True)

    c = sub.add_parser("capture", help="実機 pst-server の REST/HLS 出力を保存")
    c.add_argument("--server", default="http://localhost:8080", help="実機 pst-server のベース URL")
    c.add_argument("--channel", action="append", help="channel id (複数可)")
    c.add_argument("--board", action="append", help="BBS 板 URL (複数可)")
    c.add_argument("--thread", action="append", help="BBS スレ URL (複数可)")
    c.add_argument("--out", default="fixtures", help="出力ディレクトリ")
    c.add_argument("--hls-segments", type=int, default=3, help="保存する HLS セグメント数")
    c.set_defaults(func=capture)

    r = sub.add_parser("replay", help="保存した fixture を配信")
    r.add_argument("--fixtures", default="fixtures", help="fixture ディレクトリ")
    r.add_argument("--web", default=None, help="SPA ビルド (build) ディレクトリ")
    r.add_argument("--bind", default="127.0.0.1:8080", help="待受 host:port")
    r.set_defaults(func=replay)

    args = p.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
