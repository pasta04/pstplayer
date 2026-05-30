# ADR-0002: ライセンスとクリーンルーム実装方針

- **ステータス**: 採用 (2026-05-30)
- **決定者**: pasta04 (リポジトリオーナー)

## 背景

PCRPlayer は **GPL v3** ライセンスで配布されている。一方、PSTPlayer は **MIT License** でリリースしたい。

GPL v3 のコードを派生させた場合、派生物も GPL v3 (または互換ライセンス) で公開する必要があるため、PCRPlayer のソースコードを直接利用・移植することはできない。

ただし、PeerCast プロトコル (HTTP API、JSON-RPC API、PCP) や 2ch/したらば BBS のプロトコル (subject.txt、dat 形式、write.cgi/bbs.cgi) は **公開仕様** であり、これらを独立に再実装することは可能。

## 決定

1. **ライセンス**: MIT License を採用 (現行 `LICENSE` を維持)
2. **PCRPlayer のソースコードは直接参照しない**
   - 関数の処理ロジックや変数構成を引き写さない
   - クラス階層やデータ構造を模倣しない
3. **PCRPlayer から取得してよいのは「外形情報」のみ**
   - 機能リスト (どんな画面・機能があるか)
   - ユーザに公開されている UI 仕様 (ショートカット、画面構成)
   - README / ヘルプの記述
4. **プロトコル仕様は公開ドキュメント・公開実装から取得する**
   - PeerCast: [PeerCastStation の wiki](https://github.com/kumaryu/peercaststation/wiki) (※ GPL v3 だが、ドキュメントは仕様情報として参照可能)
   - BBS 読み取り: [unacast](https://github.com/pasta04/unacast) (同一作者・MIT 想定。仕様抽出元として OK)
   - BBS 書き込み: 公開 wiki (5ch info wiki, 2ch 型掲示板まとめ wiki) と RFC、HTTP 標準
5. **同一作者の自作コード (unacast 等) は明示的にライセンス確認の上で流用可**

## 根拠

### MIT 採用の理由

- ユーザ・開発者ともに利用障壁が低い
- 派生プロジェクト (ライブラリ抽出、組み込み等) を妨げない
- GPL の "感染" を避けたい開発者・組織も使える
- PeerCast 周辺は趣味プロジェクトが多く、ライセンス的な摩擦を最小化したい

### クリーンルーム手法を取る理由

- PCRPlayer の作者の権利を尊重する
- 法的リスクの最小化 (GPL コードのうっかり流用を防ぐ)
- 結果として、コードの構造を一から見直すきっかけになり、より良い設計が得られる

## 実装上のルール

開発時に守る具体的なガイドライン:

1. **PCRPlayer のソースをエディタで開かない** (作業者は機能リスト・スクショ・ドキュメントのみ参照)
2. **AI/エージェントへの指示時も同様**: ソースコードを読ませない。プロトコル仕様や機能仕様のドキュメントを与える
3. **コミットメッセージや PR で PCRPlayer のコード断片を引用しない**
4. **「PCRPlayer ではこう書いてあった」を根拠にコードを書かない**: 公開プロトコル仕様か、独立に書き起こした設計ドキュメントを根拠にする
5. **疑わしい場合は再設計**: 構造が偶然似てしまった場合、命名やデータ構造を変更して独立性を担保

## PeerCastStation のドキュメント参照について

PeerCastStation も GPL v3 だが、その **wiki ドキュメント** (`JSON RPC API メモ`、`PCP プロトコルメモ`、`index.txt の仕様` 等) は API/プロトコルの仕様記述であり、これは「事実情報の記述」として参照可能。

ただし、PeerCastStation の **C# ソースコード** は参照しない。

## unacast のコード参照について

unacast はリポジトリオーナーと同じ作者 (pasta04) による実装。同一作者の MIT 想定コードについては問題ないが、念のため:

- unacast の `Read5ch.ts` / `ReadSitaraba.ts` は **プロトコル仕様の抽出元** として参照する
- 直接コピーするのではなく、Rust で独立に書き起こす (言語が違うので自然と書き直しになる)
- unacast のライセンス表記がない場合、明示的に確認する

## 例外

将来、`pcoplayer` 等の MIT/Apache 互換ライセンスのプロジェクトから個別モジュールを取り込む場合は、その都度 LICENSE 表記を `NOTICE` ファイル等に追加した上で取り込み可能。

## 参考

- [PCRPlayer 配布元 (GPL v3 表記あり)](http://pecatv.s25.xrea.com/)
- [GPL v3 公式](https://www.gnu.org/licenses/gpl-3.0.html)
- [Wikipedia: Clean room design](https://en.wikipedia.org/wiki/Clean_room_design)
