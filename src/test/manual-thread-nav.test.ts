// 視聴ウィンドウ (+page.svelte) を jsdom 上にマウントし、「自分で開いた満レス
// スレが、裏の自動更新で最新スレへ引き戻されない」ことを検証する回帰テスト。
//
// 仕様: 自動選択 (コンタクト / 板からの選出 / 満レス移動) したスレは満レスで
// 最新スレへ自動移動するが、手動選択 (スレ一覧 / URL 入力) したスレは満レスでも
// 自動移動しない (過去ログを読みたいケース)。実機 QA で、手動で開いた過去の
// 満レススレが最新スレへ引き戻される不具合があった。
import { cleanup, fireEvent, render } from '@testing-library/svelte';
import { afterEach, beforeEach, expect, test, vi } from 'vitest';

// ── Tauri / 環境系のモック (viewer-count.test.ts と同じ) ─────────────
const listeners = new Map<string, (e: { payload: unknown }) => void>();

vi.mock('@tauri-apps/api/event', () => ({
	emit: vi.fn(async () => {}),
	listen: vi.fn(async (name: string, cb: (e: { payload: unknown }) => void) => {
		listeners.set(name, cb);
		return () => {};
	}),
}));

vi.mock('@tauri-apps/api/window', () => ({
	getCurrentWindow: () => ({
		setTitle: vi.fn(async () => {}),
		onFocusChanged: vi.fn(async () => () => {}),
		isFocused: vi.fn(async () => true),
		isFullscreen: vi.fn(async () => false),
		setFullscreen: vi.fn(async () => {}),
		show: vi.fn(async () => {}),
		setFocus: vi.fn(async () => {}),
		minimize: vi.fn(async () => {}),
		hide: vi.fn(async () => {}),
	}),
	LogicalSize: class {},
}));

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn(async () => {}) }));
vi.mock('$lib/windows', () => ({
	openSettings: vi.fn(),
	openYpList: vi.fn(),
	openThreadList: vi.fn(),
}));
vi.mock('$lib/shortcuts', () => ({
	installShortcuts: () => () => {},
	setAlwaysOnTop: vi.fn(async () => {}),
	setDecorations: vi.fn(async () => {}),
}));
vi.mock('$lib/notifications', () => ({ notify: vi.fn() }));
vi.mock('$lib/theme', () => ({ initTheme: () => () => {} }));
vi.mock('$lib/window-state', () => ({
	restoreMainWindowGeometry: vi.fn(async () => {}),
	watchMainWindowGeometry: vi.fn(async () => () => {}),
}));

// ── 掲示板のモック ───────────────────────────────────────────────
// 板の最大レス数は小さくして描画を軽くする。
//   200 = コンタクトのスレ (自動選択で開かれる。まだ満レスでない)
//   300 = 最新スレ (満レス時の自動移動先)
//   100 = 過去の満レススレ (ユーザーが自分で開く)
const MAX = 20;
const BASE = 'https://jbbs.shitaraba.net/bbs/read.cgi/game/1/';
const BOARD = 'https://jbbs.shitaraba.net/game/1/';
const url = (key: string) => `${BASE}${key}/`;
const counts: Record<string, number> = { '100': MAX, '200': 5, '300': 3 };

/// key ごとの取得ゲート。設定されている間、そのスレの増分取得 (= 裏の自動更新)
/// は返らずに取得中のまま止まる。
const gates: Record<string, Promise<void> | undefined> = {};

const keyOf = (u: string) => u.match(/(\d+)\/?$/)?.[1] ?? '';
function postsFor(key: string) {
	return Array.from({ length: counts[key] ?? 0 }, (_, i) => ({
		number: i + 1,
		name: '名無し',
		mail: '',
		date: '2026/09/23',
		id: '',
		body: `スレ${key} のレス ${i + 1}`,
		threadTitle: i === 0 ? `スレ${key}` : '',
	}));
}

vi.mock('$lib/api', () => {
	const noop = vi.fn(async () => {});
	return {
		CommandError: class CommandError extends Error {},
		bumpChannel: noop,
		boardUrlOf: vi.fn(async () => BOARD),
		threadUrlOf: vi.fn(async (_board: string, key: string) => url(key)),
		fetchBoardSetting: vi.fn(async () => ({ maxRes: MAX, title: '' })),
		endpointForUrl: vi.fn(async () => ({ host: 'localhost', port: 7144 })),
		fetchChannelInfo: vi.fn(async () => ({ name: 'テストch', bitrate: 0, url: url('200') })),
		fetchChannelStatus: vi.fn(async () => ({ uptime: 0, localDirects: 0, localRelays: 0 })),
		// 増分取得 (prev あり) では前回以降の分だけ返す。
		fetchThread: vi.fn(async (u: string, prev: { lastCount: number } | null) => {
			const gate = gates[keyOf(u)];
			if (gate && prev) await gate;
			const all = postsFor(keyOf(u));
			const posts = prev ? all.slice(prev.lastCount) : all;
			return [posts, { lastModified: null, lastByte: 0, lastCount: all.length, fullReload: false }];
		}),
		getCliArgs: vi.fn(async () => ({
			url: 'http://localhost:7144/pls/0123456789abcdef0123456789abcdef',
		})),
		// 自動更新間隔は明示する (既定値の変更にテストが左右されないように)。
		getConfig: vi.fn(async () => ({ bbs: { autoRefreshSec: 5 }, player: {}, peercast: {} })),
		listThreads: vi.fn(async () => [
			{ key: '300', title: 'スレ300', count: counts['300'] },
			{ key: '200', title: 'スレ200', count: counts['200'] },
			{ key: '100', title: 'スレ100', count: counts['100'] },
		]),
		playerAttach: noop,
		playerSetVideoRect: noop,
		playerLoad: noop,
		playerSetAspect: noop,
		playerSetAutoReconnect: noop,
		playerSetVolume: noop,
		playerSnapshot: vi.fn(async () => ''),
		playerStatus: vi.fn(async () => ({})),
		playerStop: noop,
		postToThread: noop,
		rememberChannelVolume: noop,
		resolveStreamUrl: vi.fn(async (u: string) => u),
		sanitizeHtml: vi.fn(async (s: string) => s),
		setConfig: noop,
		startChannelPolling: noop,
		stopChannel: noop,
		stopChannelPolling: noop,
	};
});

import * as api from '$lib/api';
import Page from '../routes/+page.svelte';

beforeEach(() => {
	counts['200'] = 5;
	delete gates['200'];
	vi.mocked(api.fetchThread).mockClear();
	vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval'] });
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

/// 表示中のスレの key (1 レス目の本文から判定)。
const shownKey = (c: HTMLElement) =>
	c.querySelector('.post')?.textContent?.match(/スレ(\d+) のレス/)?.[1] ?? '';

/// 自動選択でコンタクトのスレ (200) が開かれるまで待つ。
async function mountOnAutoThread() {
	const view = render(Page);
	await vi.waitFor(() => expect(shownKey(view.container)).toBe('200'));
	return view;
}

/// 画面内のスレッド一覧パネルを開く。
async function openPanel(c: HTMLElement) {
	const btn = c.querySelector<HTMLButtonElement>('.thread-bar-button');
	await vi.waitFor(() => expect(btn?.disabled).toBe(false));
	await fireEvent.click(btn!);
	await vi.waitFor(() => expect(c.querySelectorAll('.tl-item').length).toBe(3));
}

/// 最新スレ (300) を一度でも取得したか = 自動移動が走ったか。
const fetchedNewest = () =>
	vi.mocked(api.fetchThread).mock.calls.some(([u]) => keyOf(u as string) === '300');

test('画面内のスレ一覧から開いた満レススレは、自動更新で最新スレへ戻されない', async () => {
	const { container } = await mountOnAutoThread();
	await openPanel(container);

	const old = [...container.querySelectorAll<HTMLButtonElement>('.tl-item')].find((b) =>
		b.textContent?.includes('スレ100'),
	);
	await fireEvent.click(old!);
	await vi.waitFor(() => expect(shownKey(container)).toBe('100'));

	// 自動更新 + 満レス移動の待機 (5 秒) を十分に超えて進める。
	await vi.advanceTimersByTimeAsync(30_000);

	expect(shownKey(container)).toBe('100');
	expect(fetchedNewest()).toBe(false);
});

test('URL 入力で開いた満レススレは、最新スレへ飛ばされない', async () => {
	const { container } = await mountOnAutoThread();
	await openPanel(container);

	const input = container.querySelector<HTMLInputElement>('.tl-urlbar input')!;
	await fireEvent.input(input, { target: { value: url('100') } });
	await fireEvent.submit(container.querySelector('.tl-urlbar')!);
	await vi.waitFor(() => expect(shownKey(container)).toBe('100'));

	await vi.advanceTimersByTimeAsync(30_000);

	expect(shownKey(container)).toBe('100');
	expect(fetchedNewest()).toBe(false);
});

test('満レス移動の待機中に手動でスレを選んだら、そちらを優先する (競合)', async () => {
	const { container } = await mountOnAutoThread();

	// 自動選択中のスレが満レスになる → 次の自動更新で最新スレ探索が始まり、
	// 5 秒待機に入る。
	counts['200'] = MAX;
	await vi.advanceTimersByTimeAsync(5_000);
	await vi.waitFor(() => expect(container.querySelectorAll('.post').length).toBe(MAX));

	// 待機中にユーザーが別ウィンドウのスレ一覧から過去スレを選ぶ。
	await listeners.get('thread:selected')!({
		payload: { boardUrl: BASE, key: '100', title: 'スレ100' },
	});
	await vi.waitFor(() => expect(shownKey(container)).toBe('100'));

	// 探索の待機が明けても、ユーザーの選択を上書きしない。
	await vi.advanceTimersByTimeAsync(30_000);

	expect(shownKey(container)).toBe('100');
});

test('自動選択のスレは、満レスになれば従来どおり最新スレへ移動する', async () => {
	const { container } = await mountOnAutoThread();

	counts['200'] = MAX;
	await vi.advanceTimersByTimeAsync(30_000);

	await vi.waitFor(() => expect(shownKey(container)).toBe('300'));
});

test('裏の自動更新の取得中に手動でスレを選んでも、古い取得結果で上書きされない (競合)', async () => {
	const { container } = await mountOnAutoThread();

	// 次の自動更新 (現スレ 200 の増分取得) を取得中のまま止める。
	let release!: () => void;
	gates['200'] = new Promise<void>((r) => (release = r));
	counts['200'] = 8; // 取得が返れば新着 3 件
	await vi.advanceTimersByTimeAsync(5_000);
	await vi.waitFor(() =>
		expect(
			vi
				.mocked(api.fetchThread)
				.mock.calls.some(([u, prev]) => keyOf(u as string) === '200' && prev),
		).toBe(true),
	);

	// 取得中にユーザーが過去スレを開く。
	await listeners.get('thread:selected')!({
		payload: { boardUrl: BASE, key: '100', title: 'スレ100' },
	});
	await vi.waitFor(() => expect(shownKey(container)).toBe('100'));

	// 前のスレの取得が遅れて返ってくる。
	release();
	await vi.advanceTimersByTimeAsync(1_000);

	// 手動で開いたスレのレスだけが表示されている (前のスレのレスが混ざらない)。
	// ※ DOM だけでは不十分: 混ざるとレス番号の重複キーで描画処理ごと落ち、
	//   DOM は直前の表示のまま残る (= 見かけ上は正しく見える)。
	const bodies = [...container.querySelectorAll('.post')].map((el) => el.textContent ?? '');
	expect(bodies.length).toBe(MAX);
	expect(bodies.every((t) => t.includes('スレ100'))).toBe(true);

	// 増分状態も上書きされていないこと: 次の自動更新は、手動で開いたスレを
	// その続き (100 の既読件数) から取得する。前のスレの状態で上書きされて
	// いると、前のスレの件数から取りに行き、レスが重複・欠落する。
	vi.mocked(api.fetchThread).mockClear();
	await vi.advanceTimersByTimeAsync(5_000);
	const calls = vi.mocked(api.fetchThread).mock.calls;
	expect(calls.length).toBeGreaterThan(0);
	for (const [u, prev] of calls) {
		expect(keyOf(u as string)).toBe('100');
		expect((prev as { lastCount: number } | null)?.lastCount).toBe(MAX);
	}
});
