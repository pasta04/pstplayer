# ADR-0005: Cargo ワークスペース化と将来のリレーサーバ

- **ステータス**: 採用 (2026-05-30) — ワークスペース分離は実施。サーバ実装はフェーズ 4
- **決定者**: pasta04 (リポジトリオーナー)

## 背景

iPad / iPhone / Android などのモバイル端末から PeerCast を視聴 + コンタクト掲示板に書き込みしたいユースケースが提起された。

現状の代替手段:

- PeerCast 本体の Web ページ (`http://host:7144/`) で視聴は可能。ただし書き込み機能なし
- PSTPlayer は Tauri デスクトップアプリのため、モバイル端末では動かない

「家庭内 Linux サーバ等に **中継アプリ** を立てて、モバイル端末からブラウザでアクセスする」案を、PSTPlayer と並走させる前提で設計に組み込む。

## 決定

1. **Rust コードを Cargo ワークスペース化**
   - ルート `Cargo.toml` を workspace 定義に
   - 純ロジック (PeerCast / BBS / config / cli / util) を `crates/pst-core/` に分離
   - Tauri 固有 (commands / player / 実行ファイル) は `src-tauri/` に残す

2. **将来 `pst-server` バイナリを同一ワークスペースに追加**
   - `crates/pst-server/` (仮称) として、`axum` ベースの HTTP API + 静的ファイル配信
   - 内部で `pst-core` を再利用
   - フロントは Svelte の Web ビルドをそのまま使う

3. **サーバ実装は v1.x まで保留**
   - まず PSTPlayer Desktop の MVP (フェーズ 1) を完了 → v0.5 → v1.0
   - その後にサーバ実装をフェーズ 4 で着手

## ワークスペース構造

```
pstplayer/
├── Cargo.toml                ← workspace 定義
├── crates/
│   └── pst-core/             ← UI 非依存のロジック
│       ├── Cargo.toml
│       └── src/
│           ├── lib.rs
│           ├── peercast/
│           ├── bbs/
│           ├── config/
│           ├── cli.rs
│           ├── single_instance.rs
│           └── util/
├── src-tauri/                ← Tauri デスクトップアプリ
│   ├── Cargo.toml            ← pst-core を依存に
│   └── src/
│       ├── lib.rs
│       ├── main.rs
│       ├── commands/         ← Tauri command を pst-core にディスパッチ
│       └── player/           ← libmpv 統合
└── crates/
    └── pst-server/           ← (将来) axum サーバ
```

## モバイル対応の技術検討

### ブラウザ別の動画再生制約

| ブラウザ           | HLS                                | MP4 | WebM | FLV | TS  |
| ------------------ | ---------------------------------- | --- | ---- | --- | --- |
| iPad Safari        | ✓ ネイティブ                       | ✓   | △    | ✗   | ✗   |
| iPhone Safari      | ✓ ネイティブ                       | ✓   | △    | ✗   | ✗   |
| Android Chrome     | ⚠ hls.js 等の JS ライブラリで対応 | ✓   | ✓    | ✗   | ✗   |
| Desktop Chrome     | ⚠ hls.js                           | ✓   | ✓    | ✗   | ✗   |
| Desktop Safari     | ✓ ネイティブ                       | ✓   | △    | ✗   | ✗   |
| Desktop Firefox    | ⚠ hls.js                           | ✓   | ✓    | ✗   | ✗   |

→ **サーバが HLS で配信** すれば 1 形式でモバイル + デスクトップを網羅可能。

### HLS 提供のアプローチ

#### A. PeerCast 本体の HLS 出力を活用 (推奨)

**PeerCastStation の視聴プレイヤー設定で RTMP / HLS を選択可能**。PeerCast 本体が HLS で出してくれるなら、中継サーバは:

- BBS API のプロキシ (read/write)
- HLS URL の取り回し
- 静的フロントの配信

だけでよく、**FFmpeg のリアルタイム変換が不要**。CPU 負荷 ≒ ゼロ、レイテンシも PeerCast 本体の HLS パッケージング遅延 (数秒) だけ。

要確認: PeerCastStation のどのバージョンから HLS 対応か、HLS 出力のエンドポイント URL 形式、対応チャンネル形式 (元が H.264/AAC でないと再エンコードが必要かも)。

#### B. FFmpeg リアルタイム再パッケージ (フォールバック)

PeerCast 側で HLS が出ない場合や、元がエンコーダ非対応 (WMV9 等) のケースに備える:

- コンテナ詰め替えだけ: `ffmpeg -i http://peercast/stream/X.flv -c:v copy -c:a copy -f hls ...` → サーバ CPU ほぼゼロ
- 再エンコード必要: `-c:v libx264 -c:a aac` → 配信ビットレートの数倍の CPU

MVP では A 一択、B は配信元によっては動かない、というのが現実解。

### サーバ実装の概要 (将来計画)

```
[pst-server (axum)]
├── /api/* …………… JSON-RPC (pst-core 経由)
│   ├── /api/channel/{id}/info
│   ├── /api/channel/{id}/bump
│   ├── /api/board?url=...
│   ├── /api/thread?url=...
│   └── /api/thread/post (POST)
├── /stream/{id}.m3u8 … PeerCast 本体の HLS をプロキシ or 再パッケージ
└── /  …………………… Svelte の Web ビルド (PWA)
```

PWA 化 (manifest + Service Worker) しておけば iPhone / Android のホーム画面アイコンから独立アプリ的に起動できる。

### セキュリティ

- 家庭内 LAN のみ → 認証不要
- 外向き公開 → Basic 認証 / OAuth / WireGuard 経由 のいずれか
- BBS 投稿の元 IP は中継サーバになる (規制された場合に複数視聴者が連帯責任になる)

### スコープへの影響

ADR-0004 (PCRPlayer 互換 + オフライン系オミット) の **拡張** で、衝突はしない。
ロードマップのフェーズ 4 にあった「リモート操作 (別端末からの再生制御)」項目と統合。

## 根拠

1. **早期分離のコストが小さい**: 今 (`src-tauri/src/{peercast,bbs,...}` をそのまま `crates/pst-core/src/` に move するだけ) でやれば 1-2 時間。後でやると Tauri command との結合が深まるほどコストが上がる
2. **Tauri 非依存層のテスト性向上**: `pst-core` 単体で `cargo test` 実行できる、依存も小さくキャッシュも効く
3. **将来の B (サーバ) や C (CLI ツール) への展開を全部選択肢として残す**: 何も決め打ちにしない

## 関連 ADR

- ADR-0001: 技術スタック
- ADR-0004: 機能スコープ

## TBD (フェーズ 4 までに決める)

- PeerCastStation の HLS 出力対応バージョン / URL 形式の調査
- PWA マニフェスト設計
- 認証方式 (Basic / OAuth / なし)
- サーバ常駐方式 (systemd / Docker)
- マルチセッション対応 (複数ブラウザから同時アクセス)
- BBS Cookie の扱い (中継サーバ共有 vs ブラウザごと別管理)
