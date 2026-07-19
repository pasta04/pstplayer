# Raspberry Pi で pst-server を常駐させる

Raspberry Pi (64bit OS) を「視聴の中継 + 録画」のヘッドレスサーバとして
運用する手順。GUI (Tauri/libmpv) は使わない。

## 1. バイナリの入手

CI の `Build artifacts` ワークフローが `pst-server-linux-arm64` (tar.gz、
pst-server バイナリ + Web UI `web/` 同梱) を生成する。リリースには
`pst-server-{version}-linux-arm64.tar.gz` として添付される。

- ビルド環境: ubuntu-22.04-arm (glibc 2.35)。Raspberry Pi OS bookworm
  (glibc 2.36) 以降で追加ライブラリなしに動く (TLS は rustls 静的リンク)。

```bash
sudo mkdir -p /opt/pst-server
sudo tar xzf pst-server-linux-arm64.tar.gz -C /opt/pst-server --strip-components=1
```

## 2. 設定

初回起動時に既定パス `~/.config/PSTPlayer/pst-server.toml` が使われる
(`--config <path>` で明示指定も可)。最低限の設定:

```toml
[peercast]
# 上流 PeerCast。Windows の PeerCastStation を使うのが最小構成。
host = "192.168.x.x"
port = 7144

[recording]
enabled = true
dir = "/mnt/hdd/recordings"   # 録画先 (十分な空き容量のあるパス)
```

注意: HLS (`/hls`, iPhone Safari 向けフォールバック) は PeerCastStation
固有機能。上流に peercast-yt を使う場合、通常ブラウザの FLV 直結視聴
(`/pls` + `/stream`) は動くが HLS は使えない。

## 3. 動作確認 (手動起動)

```bash
/opt/pst-server/pst-server --bind 0.0.0.0:8080 --web /opt/pst-server/web
# 別端末のブラウザで http://<PiのIP>:8080 → YP 一覧が出れば OK
```

## 4. systemd で常駐化

`/etc/systemd/system/pst-server.service`:

```ini
[Unit]
Description=PSTPlayer pst-server (PeerCast relay / recorder / web UI)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=pi
ExecStart=/opt/pst-server/pst-server --bind 0.0.0.0:8080 --web /opt/pst-server/web --config /home/pi/.config/PSTPlayer/pst-server.toml
Restart=on-failure
RestartSec=5
# 録画でファイルを書くため、書き込み先を制限したい場合は
# ReadWritePaths= と ProtectSystem=strict を併用する。

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now pst-server
systemctl status pst-server         # 状態確認
journalctl -u pst-server -f         # ログ追尾
```

## 5. 更新

```bash
sudo systemctl stop pst-server
sudo tar xzf pst-server-linux-arm64.tar.gz -C /opt/pst-server --strip-components=1
sudo systemctl start pst-server
```
