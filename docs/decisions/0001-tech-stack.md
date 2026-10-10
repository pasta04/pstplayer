# ADR-0001: 技術スタック

- **ステータス**: 採用 (2026-05-30)
- **決定者**: pasta04 (リポジトリオーナー)

## 背景

PCRPlayer は Windows 専用の C++/MFC/DirectShow アプリケーション。クロスプラットフォーム (Windows / macOS / Linux) で同等機能を提供する PSTPlayer を新規実装する。

候補となった主な選択肢:

| 候補                  | 概略                                          |
| --------------------- | --------------------------------------------- |
| Tauri + Rust + Web UI | 軽量、Web 技術 UI、Rust の安全性              |
| Electron + TypeScript | エコシステム最大、開発速度、バイナリ重め      |
| Flutter + Dart        | 真のクロスプラットフォーム UI、動画再生が弱め |
| Qt + C++              | 元ソースに最も近い、開発の現代性は劣る        |

## 決定

**Tauri 2.x + Rust (バックエンド) + TypeScript + Web UI (フロントエンド) + libmpv (動画)** を採用する。

## 根拠

1. **配布バイナリの軽さ**: Tauri は OS の WebView を使うため 5–15MB 程度に収まる。Electron の 100MB+ と比較して PeerCast 視聴ツールとして妥当
2. **Rust のネットワーク・バイナリ処理適性**: PeerCast の HTTP API、BBS の dat パース・文字コード変換、libmpv の FFI といった本アプリの中核がすべて Rust の得意分野
3. **3 OS 対応の実装コスト**: Tauri 公式が Win/Mac/Linux のビルドツールを提供。CI matrix で同時ビルド可能
4. **前例の存在**: [pcoplayer](https://github.com/progre/pcoplayer) (同様の PCRPlayer 移植プロジェクト) が Tauri + Rust + TS で実装されている (作りかけだが構成の妥当性は確認できる)
5. **動画再生の汎用性**: libmpv (mpv の組み込み版) は FFmpeg ベースで FLV/MPEG-2 TS/WMV を含む全コンテナ対応。Windows 専用の DirectShow + FLVSplitter から脱却できる

## トレードオフ

| メリット                                   | デメリット                                                          |
| ------------------------------------------ | ------------------------------------------------------------------- |
| バイナリが小さい                           | OS 標準 WebView (Edge/WebKit/WebKitGTK) の差異を意識する必要あり    |
| Rust の型安全・並行性                      | Rust の学習・所有権による試行錯誤                                   |
| Tauri は活発に開発中、エコシステム成長中   | API が 1.x → 2.x で大きく変わった (今後も変化リスクあり)             |
| libmpv は機能豊富                          | ウィンドウ埋め込みは OS ごとに微妙にコード分岐が必要                |

## 代替案を採用しなかった理由

- **Electron**: バイナリ重い (HDD/ネットワーク帯域に優しくない)、Node.js ランタイム同梱で常駐メモリも大きい
- **Flutter**: 動画再生プラグイン (`video_player` / `fvp` 等) の品質や HTTP ストリーミング対応が libmpv ほど枯れていない。BBS の Shift_JIS / EUC-JP 処理ライブラリも Dart は手薄
- **Qt + C++**: クロスコンパイル・配布の自動化が現代的でない。libmpv 連携は可能だが、Rust ほど書き味が良くない

## 関連 ADR

- ADR-0002: クリーンルーム実装方針とライセンス
- ADR-0003: フロントエンド UI フレームワーク選定 (Svelte / React / Solid / Vue) — **未決**

## 参考

- [Tauri 公式](https://tauri.app/)
- [libmpv 公式](https://mpv.io/manual/master/#libmpv)
- [libmpv2 (Rust binding)](https://crates.io/crates/libmpv2)
- [pcoplayer](https://github.com/progre/pcoplayer)
