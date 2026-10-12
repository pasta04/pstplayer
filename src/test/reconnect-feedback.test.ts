// 視聴ウィンドウ (+page.svelte) を jsdom 上にマウントし、再接続 / 切断ボタンを
// 押したときの反応 (押した直後の見た目、再接続中の表示と PeerCast の接続状況、
// 完了の表示) を検証する。実機 QA: 押しても何も変わらず、再接続中かどうかも
// 分からなかった。
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

/// PeerCast が返すチャンネル状態 (テストごとに書き換える)。
let pcStatus = { status: 'Receiving', isReceiving: true };
/// bumpChannel の完了を待たせるためのゲート。
let releaseBump: (() => void) | null = null;

vi.mock('$lib/api', () => {
	const noop = vi.fn(async () => {});
	const status = () => ({
		...pcStatus,
		uptime: 0,
		localDirects: 1,
		localRelays: 0,
		totalDirects: 1,
		totalRelays: 0,
		isBroadcasting: false,
		isRelayFull: false,
		isDirectFull: false,
	});
	return {
		CommandError: class CommandError extends Error {},
		bumpChannel: vi.fn(
			() =>
				new Promise<void>((r) => {
					releaseBump = r;
				}),
		),
		boardUrlOf: vi.fn(async (u: string) => u),
		threadUrlOf: vi.fn(async (b: string, k: string) => `${b}${k}/`),
		fetchBoardSetting: vi.fn(async () => ({ maxRes: 1000, title: '' })),
		endpointForUrl: vi.fn(async () => ({ host: 'localhost', port: 7144 })),
		fetchChannelInfo: vi.fn(async () => ({ name: 'テストch', bitrate: 1000, url: '' })),
		fetchChannelStatus: vi.fn(async () => status()),
		fetchThread: vi.fn(async () => [[], { lastModified: null, lastByte: 0, lastCount: 0 }]),
		getCliArgs: vi.fn(async () => ({
			url: 'http://localhost:7144/pls/0123456789abcdef0123456789abcdef',
		})),
		getConfig: vi.fn(async () => ({ bbs: {}, player: {}, peercast: {} })),
		listThreads: vi.fn(async () => []),
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

import Page from '../routes/+page.svelte';

beforeEach(() => {
	pcStatus = { status: 'Receiving', isReceiving: true };
	releaseBump = null;
	vi.useFakeTimers({
		toFake: ['setTimeout', 'clearTimeout', 'setInterval', 'clearInterval', 'Date'],
	});
});
afterEach(() => {
	cleanup();
	vi.useRealTimers();
});

const statusText = (c: HTMLElement) => c.querySelector('.reconnect')?.textContent?.trim() ?? '';
const buttons = (c: HTMLElement) => {
	const [bump, stop] = c.querySelectorAll<HTMLButtonElement>('.s-actions button');
	return { bump, stop };
};

async function mountPlaying() {
	const view = render(Page);
	await vi.waitFor(() => expect(view.container.querySelector('.s-actions button')).not.toBeNull());
	return view.container;
}

test('再接続ボタン: 押した直後から処理中になり、PeerCast の状況を出し、完了を知らせる', async () => {
	const c = await mountPlaying();
	const { bump, stop } = buttons(c);

	// 上流を切り替え中 = PeerCast はソース探索中。
	pcStatus = { status: 'Searching', isReceiving: false };
	await fireEvent.click(bump);

	// 押した直後: ボタンが処理中の見た目になり、二重押しできない。
	await vi.waitFor(() => expect(statusText(c)).toContain('再接続中…'));
	expect(bump.classList.contains('busy')).toBe(true);
	expect(bump.disabled).toBe(true);
	expect(stop.disabled).toBe(true);
	expect(c.querySelector('.reconnect .spinner')).not.toBeNull();

	// 1 秒ごとに PeerCast の状況を取り直して表示する。
	await vi.advanceTimersByTimeAsync(1_000);
	await vi.waitFor(() => expect(statusText(c)).toContain('PeerCast: ソース探索中'));

	// Bump の要求が返って再生を読み込み直した。PeerCast はまだ探索中なので未完了。
	releaseBump!();
	await vi.advanceTimersByTimeAsync(0);
	listeners.get('player:file_loaded')!({ payload: { was_reconnect: false, attempt: 0 } });
	await vi.advanceTimersByTimeAsync(1_000);
	expect(statusText(c)).toContain('再接続中…');

	// PeerCast が受信を再開したら完了を知らせ、スピナーを止める。
	pcStatus = { status: 'Receiving', isReceiving: true };
	await vi.advanceTimersByTimeAsync(1_000);
	await vi.waitFor(() => expect(statusText(c)).toBe('✓ 再接続しました'));
	expect(c.querySelector('.reconnect .spinner')).toBeNull();
	expect(bump.disabled).toBe(false);

	// 完了の表示はしばらくすると消える。
	await vi.advanceTimersByTimeAsync(5_000);
	expect(c.querySelector('.reconnect')).toBeNull();
});

test('再接続ボタン: 時間内に受信を再開しなければ、その時点の PeerCast の状況を出して終える', async () => {
	const c = await mountPlaying();
	pcStatus = { status: 'Searching', isReceiving: false };
	await fireEvent.click(buttons(c).bump);
	releaseBump!();
	await vi.advanceTimersByTimeAsync(61_000);
	await vi.waitFor(() =>
		expect(statusText(c)).toBe('⚠ 再接続できていません (PeerCast: ソース探索中)'),
	);
	expect(c.querySelector('.reconnect .spinner')).toBeNull();
});

test('切断ボタン: 処理中の見た目になり、切断したことを知らせる', async () => {
	const c = await mountPlaying();
	await fireEvent.click(buttons(c).stop);
	await vi.waitFor(() => expect(statusText(c)).toBe('■ 切断しました'));
});

test('自動再接続中も PeerCast の状況を添える', async () => {
	const c = await mountPlaying();
	pcStatus = { status: 'Connecting', isReceiving: false };
	listeners.get('player:reconnecting')!({
		payload: { attempt: 1, max: 10, delay_sec: 1, end_file_reason: 'eof' },
	});
	await vi.advanceTimersByTimeAsync(1_000);
	await vi.waitFor(() => expect(statusText(c)).toContain('PeerCast: 接続中'));
	expect(statusText(c)).toContain('自動再接続中… 試行 1/10');

	listeners.get('player:file_loaded')!({ payload: { was_reconnect: true, attempt: 1 } });
	await vi.waitFor(() => expect(statusText(c)).toBe('✓ 再接続しました (試行 1 回目)'));
});
