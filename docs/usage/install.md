# インストール

[GitHub Releases](https://github.com/pasta04/pstplayer/releases) から
お使いの OS のファイルをダウンロードしてください。

> v0.1.0 リリース前 (本ドキュメント執筆時点) は GitHub Actions の
> Artifacts (各コミットごとに 14 日間保持) からも取得できます。

## Windows

形式: **portable ZIP**

1. `pstplayer-{version}-windows-x64-portable.zip` をダウンロード
2. 適当なフォルダに展開 (例: `C:\Apps\PSTPlayer\`)。中身は以下:
   - `pstplayer.exe` — 本体
   - `libmpv-2.dll` — 動画再生ライブラリ
   - `README.txt`
3. `pstplayer.exe` をダブルクリックして起動

### 初回起動時の警告

Windows SmartScreen が「WindowsによってPCが保護されました」と表示
することがあります (未署名のため)。

1. 「**詳細情報**」をクリック
2. 「**実行**」ボタンを押す

> 開発者署名証明書は持っていないため、当面この手順が必要です。

### libmpv-2.dll が見つからない

ZIP に同梱されている `libmpv-2.dll` は `pstplayer.exe` と**同じ
フォルダ**に置く必要があります。デスクトップショートカットなどから
起動する場合、ショートカットの「作業フォルダ」が exe のあるフォルダに
なっているかを確認してください。

---

## macOS

形式: **.app.zip** (Frameworks に libmpv を同梱、Homebrew インストール
不要)

1. `pstplayer-{version}-macos-universal.app.zip` をダウンロード
2. 展開すると `PSTPlayer.app` が出る
3. `/Applications/` にドラッグ&ドロップ
4. 初回起動時は **Cmd + クリック → 開く** で「開発元未確認」の警告を
   通過 (ダブルクリックだと開けません)

### Gatekeeper の隔離属性を外す (任意)

毎回警告を出したくない場合は Terminal で:

```bash
xattr -dr com.apple.quarantine /Applications/PSTPlayer.app
```

> Apple Developer Program 加入 + 公証対応は v1.0 以降に検討。

---

## Linux

形式: **.deb / .rpm / .AppImage** から選択

### Ubuntu / Debian 系 (.deb)

```bash
sudo apt install ./pstplayer-{version}-linux-x64.deb
# 依存 (libmpv2, libwebkit2gtk-4.1-0 等) は apt が自動解決
pstplayer            # コマンドラインから起動可
```

### Fedora / RHEL 系 (.rpm)

```bash
sudo dnf install ./pstplayer-{version}-linux-x64.rpm
pstplayer
```

### AppImage (どのディストリでも)

```bash
chmod +x pstplayer-{version}-linux-x64.AppImage
./pstplayer-{version}-linux-x64.AppImage
```

ただし AppImage 版は **システムの libmpv に依存** します。事前に
入っていない場合:

```bash
sudo apt install libmpv2   # Debian / Ubuntu
sudo dnf install mpv-libs  # Fedora
```

### Wayland 注意

Wayland セッションでは libmpv のネイティブウィンドウ埋め込み
(wid) が利用できないため、現在のところ動画が表示されません。
X11 セッションで起動してください (GDM のログイン画面で歯車から選択)。

---

## 起動確認

正常に起動すると、メインウィンドウが開き、初回は **YP チャンネル一覧
ウィンドウ** か **設定ウィンドウ** が自動で前面に出ます:

- YP が出た場合 → PeerCast 本体に接続できています。次は [基本操作](basic.md) へ
- 設定が出た場合 → PeerCast 本体に接続できていません。次は [初回セットアップ](first-setup.md) へ
