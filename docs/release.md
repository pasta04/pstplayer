# リリース / 配布方針

PSTPlayer の配布バイナリのバージョニング、命名、リリース手順、
アイコン仕様、将来計画の単一情報源。

実装の現況とのギャップは「未実装」マークで明示する。仕様 (この
ドキュメント) と CI ワークフロー (`.github/workflows/build.yml`) は
常に整合させる。

---

## 1. バージョニング

### 1.1 採用スキーム

[SemVer 2.0.0](https://semver.org/lang/ja/) (`MAJOR.MINOR.PATCH`)。

- **MAJOR**: 互換性のない変更 (TOML スキーマ破壊、CLI 引数の意味変更など)
- **MINOR**: 後方互換な機能追加 (新ホットキー、新タブ、対応 BBS 追加など)
- **PATCH**: 後方互換なバグ修正のみ

プレリリースは `-rc.N` / `-beta.N` を付けて表記する (`v0.2.0-rc.1`)。

### 1.2 マイルストーン対応

| バージョン | フェーズ                          | 内容                                                |
| ---------- | --------------------------------- | --------------------------------------------------- |
| v0.0.x     | 開発中 (現在)                     | 公開リリースしない                                  |
| v0.1.0     | フェーズ 1 完了 (MVP)             | 3 OS で「再生 + したらば視聴 + 書き込み」が成立     |
| v0.5.0     | フェーズ 2 完了                   | PCRPlayer 主要機能の網羅、日常使いに耐える状態      |
| v1.0.0     | フェーズ 3 完了                   | 安定化 + ドキュメント + コード署名 / 公証 (任意)    |

### 1.3 単一情報源 (SSOT)

バージョン番号は **`Cargo.toml` (workspace) を SSOT** とし、その他の
ファイルは bump スクリプトで同期する。手動同期する場合は以下:

| ファイル                          | 設定行                  | 同期方針 |
| --------------------------------- | ----------------------- | -------- |
| `Cargo.toml` (workspace.package) | `version = "X.Y.Z"`    | SSOT     |
| `package.json`                    | `"version": "X.Y.Z"`    | 追従     |
| `src-tauri/tauri.conf.json`       | `"version": "X.Y.Z"`    | 追従     |

`pst-core` / `pstplayer` クレートは `version.workspace = true` で
SSOT を自動継承する。

---

## 2. ブランチ・タグ運用

### 2.1 ブランチ

- **`main`**: 常に「リリース可能な状態」を保つ。ここから直接タグを切る
- 機能ブランチは PR 経由でマージ
- リリース専用の長命ブランチ (release/x.y) は持たない (small project)

### 2.2 タグ命名

- 形式: `v{semver}` (例: `v0.1.0`, `v0.2.0-rc.1`)
- リリースタグは **annotated tag** (`git tag -a`) を使う
- タグの prefix `v` 必須 (CI ワークフローが `v*` でフィルタするため)

### 2.3 リリース手順 (将来、実装後)

```bash
# 1. ローカル
vim Cargo.toml package.json src-tauri/tauri.conf.json   # version を bump
cargo update -w                                          # Cargo.lock 反映
git commit -am "chore: release v0.1.0"
git tag -a v0.1.0 -m "MVP release"
git push origin main v0.1.0

# 2. GitHub Actions
#    - build.yml が v* タグトリガで起動 (★未実装)
#    - artifacts を生成し、release.yml が GitHub Releases にアップロード (★未実装)

# 3. 公開後
#    - リリースページの本文を手で整える (Highlights / Known issues)
#    - 必要なら release notes に CHANGELOG を貼る
```

---

## 3. 配布物の命名規則

Tauri bundler が出力するファイル名と、PSTPlayer として統一したい
ファイル名にはズレがある。将来的に **`pstplayer-{version}-{os}-{arch}.{ext}`**
形式へ rename して GitHub Releases に上げる。

### 3.1 OS 別の中身と最終配布名

| OS      | バンドル形式             | bundler 出力例                       | 配布リネーム後                              |
| ------- | ------------------------ | ------------------------------------ | ------------------------------------------- |
| Linux   | `.deb` (apt 系)          | `pstplayer_0.1.0_amd64.deb`          | `pstplayer-0.1.0-linux-x64.deb`             |
| Linux   | `.rpm` (rpm 系)          | `pstplayer-0.1.0-1.x86_64.rpm`       | `pstplayer-0.1.0-linux-x64.rpm`             |
| Linux   | `.AppImage` (portable)   | `pstplayer_0.1.0_amd64.AppImage`     | `pstplayer-0.1.0-linux-x64.AppImage`        |
| macOS   | `.app` (Frameworks 同梱) | `PSTPlayer.app/`                     | `pstplayer-0.1.0-macos-universal.app.zip`   |
| Windows | portable zip             | `pstplayer.exe + libmpv-2.dll`       | `pstplayer-0.1.0-windows-x64-portable.zip`  |

### 3.2 Windows ポータブル ZIP の中身

- `pstplayer.exe` (cargo build --release から)
- `libmpv-2.dll` (shinchiro/mpv-winbuild-cmake 由来、LGPL-2.1)
- `README.txt` (`src-tauri/distrib/windows-portable-readme.txt`)
- (将来) `LICENSE.txt`, `THIRD-PARTY.txt`

Windows installer (NSIS/MSI) は MVP では作らない。代わりに ZIP 配布。
理由: NSIS bundler が libmpv のリンクで頻繁にコケる + ユーザーの大半
が一時的に試したいだけというニーズに合致しないため。需要が出たら
再検討。

### 3.3 macOS .app の中身

```
PSTPlayer.app/
├── Contents/
│   ├── Info.plist
│   ├── MacOS/
│   │   └── pstplayer
│   ├── Frameworks/
│   │   ├── libmpv.2.dylib          ← brew --prefix mpv からコピー
│   │   └── (libmpv の依存 dylib も otool 解析で再帰コピー)
│   └── Resources/
│       └── icon.icns
```

`brew install mpv` をユーザーに要求しないために Frameworks/ 同梱は
必須。`.app.zip` で配布 (.dmg は cosmetic な ON/OFF 判断、現状は無し)。

### 3.4 Linux .deb / .rpm の依存

- `libmpv2` (mpv のシステムライブラリに依存。AppImage では同梱)
- `libwebkit2gtk-4.1-0`
- `libsoup-3.0-0`

依存はパッケージマネージャ任せ。`.AppImage` は libmpv の所在を
ユーザー環境に依存させたくないので `LD_LIBRARY_PATH` で同梱版を
優先する (将来的に対応、現状の AppImage はシステム libmpv 前提)。

---

## 4. アイコン仕様

### 4.1 現状

**v0.0.x: 専用アイコン (メガホン + 放射波) に差し替え済**。配色は
`#1e2126` (ダーク背景) + `#ff8a3d` (アクセントオレンジ、スレッド
タイトル帯と同色) + `#f3f5f7` (放射波の白)。`src-tauri/icons/icon.png`
を 512×512 ベースとし、`tauri icon` で各サイズ + `.ico` + `.icns` を
生成。Microsoft Store 用 (`Square*.png`) と iOS/Android はデスクトップ
専用なので生成しても `.gitignore` で除外している。

### 4.2 必須サイズと形式

`src-tauri/tauri.conf.json` の `bundle.icon` 配列で指定:

| ファイル                      | 用途                       |
| ----------------------------- | -------------------------- |
| `icons/32x32.png`             | Linux 小サイズ             |
| `icons/128x128.png`           | Linux 通常                 |
| `icons/128x128@2x.png`        | Linux Retina (= 256px)     |
| `icons/icon.icns`             | macOS bundle               |
| `icons/icon.ico`              | Windows exe / bundle       |
| `icons/icon.png` (512×512)    | Tauri generate のソース    |

Windows Store 用の `SquareNxN.png` 群は使わない (Store 配布しない)。
将来 Microsoft Store 対応するなら再評価。

### 4.3 デザイン指針 (採用版)

- PeerCast 本家のメガホンモチーフを継承
  (再生クライアントとしての一貫性のため)
- 「再生」を強調するため、メガホン口から放射波 2 本を出すシルエット
  構成。タスクバー / Dock の 16-32px でも識別可能
- PCRPlayer 由来のモチーフは**使用不可** (GPL かつ著作物のため)
- 配色: ダーク基調 + アクセント 1 色 (ライトテーマ移行時にも視認性確保)
- スタイル: フラット / 単純な幾何形 (アイコンサイズ縮小時の崩れ回避)

### 4.4 制作フロー

1. 512×512 png を 1 枚作る (`src-tauri/icons/icon.png`)
2. `cd src-tauri && npx tauri icon icons/icon.png` で全サイズ +
   `.ico` + `.icns` を一括生成
3. デスクトップに不要な iOS / Android / Microsoft Store 用は
   `src-tauri/.gitignore` で除外済 (再生成のたび追従するので操作不要)
4. `src-tauri/tauri.conf.json` の `bundle.icon` で参照しているファイル
   が揃っていることを確認してコミット

---

## 5. ライセンス / サードパーティ表記

### 5.1 配布物に同梱

- `LICENSE` (本体: MIT)
- `THIRD-PARTY.md` (依存ライブラリのライセンス一覧、`cargo-about`
  で release ジョブ中に都度生成 → `staging/THIRD-PARTY-{version}.md`
  として GitHub Releases に同梱)

  ローカルで生成したい時:

  ```bash
  cargo install cargo-about --locked --features cli   # 初回のみ
  cargo about generate about.hbs > THIRD-PARTY.md
  ```

  設定は `about.toml` (許容ライセンス一覧)、テンプレートは
  `about.hbs`。

### 5.2 注意が必要なもの

| 依存                              | ライセンス  | 配布上の扱い                                 |
| --------------------------------- | ----------- | -------------------------------------------- |
| libmpv (`libmpv.dll` / `.dylib`) | LGPL-2.1+   | 動的リンク必須。差し替え可能性を確保 (現状 OK) |
| ffmpeg (libmpv が依存)            | LGPL / GPL  | 配布する libmpv のビルド構成に応じて要確認   |
| shinchiro/mpv-winbuild (Windows)  | 配布特例    | ZIP 内に出典 URL を README に記載            |

GPL コンポーネントの混入は将来的な厳禁事項 (PSTPlayer 本体は MIT)。
Windows libmpv ビルドの ffmpeg が GPL 構成になっていないか毎回確認。

---

## 6. CI ワークフローとの対応

### 6.1 現状の `.github/workflows/build.yml`

- トリガ: `push` (main + 開発ブランチ + `v*` タグ) / `workflow_dispatch`
- ジョブ: 3 OS マトリクスで artifacts 生成 (14 日保持)
- リリース連携: **タグ駆動で release ジョブが動く** (詳細は §6.2)

### 6.2 release ジョブ (実装済)

`build.yml` の末尾に `release` ジョブを追加。`startsWith(github.ref,
'refs/tags/v')` でフィルタしているのでブランチ push では走らない。

実行内容:

1. `actions/download-artifact@v4` で 3 OS の artifacts を `artifacts/`
   配下に展開
2. タグ名から `VERSION` を抽出し、`-rc.*` / `-beta.*` / `-alpha.*` が
   含まれていれば `prerelease=true`
3. 配布物を `pstplayer-{version}-{os}-{arch}.{ext}` にリネーム
   - Linux: `.deb` / `.AppImage` / `.rpm` をそのまま rename
   - macOS: `.app` ディレクトリは zip 化して 1 ファイルに
   - Windows: portable ディレクトリを `pstplayer-{ver}-windows-x64-portable`
     に rename してから zip
4. `sha256sum * > SHA256SUMS.txt` でチェックサム集約
5. `gh release create "$TAG" staging/* --generate-notes --title "PSTPlayer $TAG"`
   (prerelease の時は `--prerelease` を追加)

### 6.3 (任意) 将来のロジック分割

build と release を 1 ファイルにまとめている。将来 release 周りが
複雑化したら `release.yml` を `workflow_run` で別建てする選択肢が
ある。現状はシンプルさを優先して 1 ファイル運用。

---

## 7. 将来計画 (フェーズ 3 以降)

| 項目                    | 対応案                                                  | 優先度 |
| ----------------------- | ------------------------------------------------------- | ------ |
| Windows コード署名      | Sigstore (`signtool` + 自己署名でも可)、有償 EV は見送り | 中    |
| macOS Notarization      | Apple Developer 登録 + notarytool                       | 中    |
| 自動更新                | Tauri Updater (`updater.json` を Pages でホスト)        | 中    |
| Linux Flatpak           | Flathub 登録                                            | 低    |
| Microsoft Store         | MSIX パッケージ                                         | 低    |
| Homebrew Cask           | tap を作る                                              | 低    |
| Winget                  | manifest 投稿                                           | 中    |

これらは v1.0 以降に都度判断。MVP / v0.5 では GitHub Releases から
直接ダウンロードする運用で十分。

---

## 8. リリース時のチェックリスト (運用前確認用)

リリースタグを切る前に以下を全部クリアすること。

- [ ] `cargo fmt --all -- --check` クリア
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` クリア
- [ ] `cargo test --workspace` 全 pass
- [ ] `npm run check` / `npm run lint` クリア
- [ ] バージョン番号を 3 ファイル (Cargo.toml / package.json / tauri.conf.json) 全部 bump
- [ ] `cargo update -w` で Cargo.lock 反映
- [ ] 3 OS で実際にバイナリを動かして起動・PeerCast 視聴・BBS 投稿を確認
- [ ] アイコンが PSTPlayer 専用のものに差し替え済み (v0.1.0 以降)
- [ ] CHANGELOG / リリースノート下書き準備
- [ ] LICENSE と THIRD-PARTY が最新の依存に追従

---

## 9. 関連ドキュメント

- [`docs/roadmap.md`](roadmap.md) — フェーズ別 TODO
- [`docs/architecture.md`](architecture.md) — システム構成
- [`docs/decisions/0001-tech-stack.md`](decisions/0001-tech-stack.md) — Tauri / Rust / Web UI 採用根拠
- [`docs/decisions/0002-license-clean-room.md`](decisions/0002-license-clean-room.md) — MIT + クリーンルーム
- [`.github/workflows/build.yml`](../.github/workflows/build.yml) — CI 実装
