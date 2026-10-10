PSTPlayer (Windows portable)

使い方:
  1. このフォルダ内の pstplayer.exe をダブルクリック
  2. YP 一覧 (ハブ) が表示されます
  3. チャンネルをダブルクリックで視聴 / 右クリックから録画 /
     お気に入りルールで自動録画

録画について:
  録画はアプリ内蔵の録画サーバがバックグラウンドで行います
  (ウィンドウ無し・再生無しで、配信を受信してそのまま保存)。別途サーバを
  起動する必要はありません。録画中にアプリを閉じようとすると確認が出ます。
  保存先は「設定 → プレイヤー → 録画の保存先」(空なら exe 隣の recordings\)。

同梱物:
  pstplayer.exe   ... 本体 (GUI。録画サーバを内蔵)
  pst-server.exe  ... スタンドアロン録画サーバ (任意・上級者向け)
  web\            ... pst-server.exe 用のブラウザ UI
  libmpv-2.dll    ... 動画再生エンジン (削除/移動しないでください)

pst-server.exe (任意・上級者向け):
  アプリを閉じても録画を続けたい / 常駐録画したい、または LAN・スマホの
  ブラウザから視聴・録画したい場合に使います。コマンドプロンプトで:
    pst-server.exe --bind 0.0.0.0:8080
  を実行し、ブラウザで http://<このPCのIP>:8080/hub を開くと視聴・録画
  できます (隣の web\ を自動で配信します)。設定ファイルは
  %APPDATA%\PSTPlayer\config\pst-server.toml (例は GitHub の
  docs/usage/pst-server.example.toml を参照)。
  ※ このサーバを常用するときは、アプリ側の「設定 → ハブ → pst-server URL」
     にその URL を入れてください (内蔵サーバより優先され、内蔵は起動しません)。

事前準備:
  Microsoft Edge WebView2 Runtime
    Windows 11 は標準で入っています。Windows 10 で入っていない場合は
    https://developer.microsoft.com/microsoft-edge/webview2/ から入手。

  PeerCast 本体 (PeerCastStation など) も別途必要です:
    http://www.pecastation.org/

ライセンス:
  PSTPlayer は MIT。libmpv-2.dll は LGPL (LGPL ビルドの場合) または
  GPL (デフォルトビルドの場合)。詳細は
  https://github.com/pasta04/pstplayer
