// 視聴画面のレス一覧 (.posts) を最下部に追従させるかの判定。
//
// 以前は「取得直前に最下部付近にいたか」を更新のたびにスナップショットし、
// それだけで新着レスの自動スクロール可否を決めていた。この方式だと、一度でも
// 最下部から離れると以後ずっと「最下部にいない」と判定され、手動で最下部に
// 戻すまで自動スクロールが止まり続けた (実機 QA)。離れる原因は複数あった:
//
// - 5 分ごとの定期リシンク (全件取り直し) は置換経路に入り、新着が含まれて
//   いてもスクロールしなかった
// - スクロールし終えた後に内容の高さが増えた (HTML 表示の後追い差し替え等)
// - 書き込み欄が伸びる / ペインやウィンドウのリサイズで一覧が縮んだ
// - スクロールアニメの途中で wheel が来て止まった (deltaY=0 でも止めていた)
// - BBS ペインを隠して再表示すると一覧が先頭から始まった
//
// ここでは「追従する意思」を状態として持ち、ユーザーが自分で上へスクロール
// したときだけ解除し、最下部に戻ったら再開する。位置がずれても意思は保たれる
// ので、次の更新や大きさの変化で最下部へ貼り直される。

/** これ以内なら最下部にいるとみなす (px)。 */
export const NEAR_BOTTOM_PX = 24;

export interface ScrollMetrics {
	scrollTop: number;
	scrollHeight: number;
	clientHeight: number;
}

/** 最下部までの残り距離 (px)。 */
export function gapToBottom(m: ScrollMetrics): number {
	return m.scrollHeight - m.clientHeight - m.scrollTop;
}

export function isNearBottom(m: ScrollMetrics, px = NEAR_BOTTOM_PX): boolean {
	return gapToBottom(m) <= px;
}

/**
 * スクロールイベント後の追従状態。
 *
 * - 最下部付近に来たら追従を再開する。
 * - 上方向へ動いて最下部から離れたら追従をやめる (過去レスを読みに行った)。
 *   ただし自前のスクロールアニメ中の上方向移動は、ブラウザのスクロール
 *   アンカリング等による補正でありユーザー操作ではないので無視する。
 *   アニメは下方向にしか動かさず、ユーザーが上へ動かすときは入力イベント
 *   (上向き wheel / スクロールバーのドラッグ / 上へのキー) で先にアニメを
 *   止めているので、ユーザー操作による上方向移動は必ず animating=false で届く。
 * - それ以外 (下方向への移動など) は変えない。
 */
export function followAfterScroll(s: {
	follow: boolean;
	nearBottom: boolean;
	movedUp: boolean;
	animating: boolean;
}): boolean {
	if (s.nearBottom) return true;
	if (s.movedUp && !s.animating) return false;
	return s.follow;
}

export type RepinAction = 'none' | 'smooth' | 'jump';

/**
 * 内容やコンテナの大きさが変わったとき、最下部へ貼り直すか。
 *
 * 実行中のアニメは毎フレーム目標を測り直すので任せる。大きく離れている
 * (ペインを再表示して先頭から始まった等) ときはアニメで流さず即座に飛ぶ。
 */
export function repinAction(s: {
	follow: boolean;
	enabled: boolean;
	animating: boolean;
	gap: number;
	clientHeight: number;
}): RepinAction {
	if (!s.enabled || !s.follow || s.animating) return 'none';
	if (s.gap <= 1) return 'none';
	return s.gap > s.clientHeight * 2 ? 'jump' : 'smooth';
}

/**
 * wheel で自前のスクロールアニメを止めるか。上方向 (過去レスへ) だけ止める。
 * 下方向は最下部へ向かうアニメと向きが同じなので止めない。deltaY=0 (横ホイール
 * など) で止めると、動いてもいないのにアニメが途中で終わってしまう。
 */
export function wheelCancelsAnimation(deltaY: number): boolean {
	return deltaY < 0;
}

/** 上方向へスクロールするキー。押されたらアニメを止める。 */
export const SCROLL_UP_KEYS: ReadonlySet<string> = new Set(['PageUp', 'ArrowUp', 'Home']);
