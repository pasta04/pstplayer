// レス一覧の最下部追従 (follow-bottom.ts) の回帰テスト。
// 「一度最下部から離れると自動スクロールが二度と効かない」ラッチの再発防止。
import { describe, expect, test } from 'vitest';

import {
	followAfterScroll,
	gapToBottom,
	isNearBottom,
	NEAR_BOTTOM_PX,
	repinAction,
	SCROLL_UP_KEYS,
	wheelCancelsAnimation,
} from '$lib/follow-bottom';

describe('isNearBottom / gapToBottom', () => {
	test('最下部からの距離で判定する', () => {
		const m = { scrollTop: 700, scrollHeight: 1000, clientHeight: 300 };
		expect(gapToBottom(m)).toBe(0);
		expect(isNearBottom(m)).toBe(true);
		expect(isNearBottom({ ...m, scrollTop: 700 - NEAR_BOTTOM_PX })).toBe(true);
		expect(isNearBottom({ ...m, scrollTop: 700 - NEAR_BOTTOM_PX - 1 })).toBe(false);
	});

	test('スクロールできない (内容が短い) ときは常に最下部', () => {
		expect(isNearBottom({ scrollTop: 0, scrollHeight: 200, clientHeight: 300 })).toBe(true);
	});
});

describe('followAfterScroll', () => {
	const base = { follow: true, nearBottom: false, movedUp: false, animating: false };

	test('ユーザーが上へスクロールして最下部から離れたら追従をやめる', () => {
		expect(followAfterScroll({ ...base, movedUp: true })).toBe(false);
	});

	test('最下部に戻ったら追従を再開する', () => {
		expect(followAfterScroll({ ...base, follow: false, nearBottom: true })).toBe(true);
	});

	test('下方向への移動や位置のずれだけでは追従をやめない (ラッチの根本)', () => {
		// 以前は最下部から離れた時点で判定が偽になり、以後ずっと止まっていた。
		expect(followAfterScroll({ ...base, movedUp: false })).toBe(true);
	});

	test('アニメ中の上方向移動 (スクロールアンカリング等の補正) では追従をやめない', () => {
		expect(followAfterScroll({ ...base, movedUp: true, animating: true })).toBe(true);
	});

	test('追従していないときは下方向に動いても最下部に着くまで再開しない', () => {
		expect(followAfterScroll({ ...base, follow: false })).toBe(false);
	});
});

describe('repinAction', () => {
	const base = { follow: true, enabled: true, animating: false, gap: 60, clientHeight: 300 };

	test('追従中に最下部から離れたら貼り直す (後から伸びた / 一覧が縮んだ)', () => {
		expect(repinAction(base)).toBe('smooth');
	});

	test('大きく離れている (ペイン再表示で先頭から始まった等) ときは即座に飛ぶ', () => {
		expect(repinAction({ ...base, gap: 5000 })).toBe('jump');
	});

	test('追従していない / 設定で無効 / アニメ中 / ずれていない ときは何もしない', () => {
		expect(repinAction({ ...base, follow: false })).toBe('none');
		expect(repinAction({ ...base, enabled: false })).toBe('none');
		expect(repinAction({ ...base, animating: true })).toBe('none');
		expect(repinAction({ ...base, gap: 1 })).toBe('none');
	});
});

describe('アニメを止める入力', () => {
	test('wheel は上方向だけ止める (deltaY=0 では止めない)', () => {
		expect(wheelCancelsAnimation(-100)).toBe(true);
		expect(wheelCancelsAnimation(100)).toBe(false);
		expect(wheelCancelsAnimation(0)).toBe(false);
	});

	test('上方向へのスクロールキー', () => {
		for (const k of ['PageUp', 'ArrowUp', 'Home']) expect(SCROLL_UP_KEYS.has(k)).toBe(true);
		for (const k of ['PageDown', 'ArrowDown', 'End', 'Enter']) {
			expect(SCROLL_UP_KEYS.has(k)).toBe(false);
		}
	});
});
