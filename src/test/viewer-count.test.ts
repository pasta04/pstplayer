// 視聴ウィンドウ (+page.svelte) を jsdom 上にマウントし、スレッドを読み込んだ
// ときに「スレタイ横のレス数」が反映されるかを検証する (A1 回帰テスト)。
// 実機 QA でレス数が 0 のまま固着していたバグを Web 側で再現/検証する。
import { render, cleanup } from '@testing-library/svelte';
import { afterEach, expect, test, vi } from 'vitest';

// ── Tauri / 環境系のモック ───────────────────────────────────────────
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

const POSTS = [
	{
		number: 1,
		name: '名無し',
		mail: '',
		date: '2026/06/15',
		id: '',
		body: '1 レス目',
		threadTitle: 'テストスレ',
	},
	{
		number: 2,
		name: '名無し',
		mail: 'sage',
		date: '2026/06/15',
		id: '',
		body: '2 レス目',
		threadTitle: '',
	},
	{
		number: 3,
		name: '名無し',
		mail: '',
		date: '2026/06/15',
		id: '',
		body: '3 レス目',
		threadTitle: '',
	},
];

vi.mock('$lib/api', () => {
	const noop = vi.fn(async () => {});
	return {
		CommandError: class CommandError extends Error {},
		bumpChannel: noop,
		boardUrlOf: vi.fn(async (u: string) => u),
		threadUrlOf: vi.fn(async (board: string, key: string) => `${board}${key}/`),
		fetchBoardSetting: vi.fn(async () => ({ maxRes: 1000, title: '' })),
		endpointForUrl: vi.fn(async () => ({ host: 'localhost', port: 7144 })),
		fetchChannelInfo: vi.fn(async () => ({ name: '', bitrate: 0 })),
		fetchChannelStatus: vi.fn(async () => ({ uptime: 0, localDirects: 0, localRelays: 0 })),
		fetchThread: vi.fn(async () => [POSTS, { lastModified: null, lastByte: 0, lastCount: 3 }]),
		firstFavoriteMatch: vi.fn(() => null),
		getCliArgs: vi.fn(async () => ({})),
		getConfig: vi.fn(async () => ({ bbs: {}, player: {}, peercast: {} })),
		getHistory: vi.fn(async () => []),
		listThreads: vi.fn(async () => []),
		playerAttach: noop,
		playerSetVideoRect: noop,
		playerLoad: noop,
		playerRecordPath: vi.fn(async () => null),
		playerRecordStart: noop,
		playerRecordStop: noop,
		playerSetAspect: noop,
		playerSetAutoReconnect: noop,
		playerSetVolume: noop,
		playerSnapshot: vi.fn(async () => ''),
		playerStatus: vi.fn(async () => ({})),
		playerStop: noop,
		postToThread: noop,
		pushHistory: noop,
		resolveStreamUrl: vi.fn(async (u: string) => u),
		sanitizeHtml: vi.fn(async (s: string) => s),
		setConfig: noop,
		startChannelPolling: noop,
		stopChannel: noop,
		stopChannelPolling: noop,
	};
});

// マウント後に import する (モック適用後)。
import Page from '../routes/+page.svelte';

afterEach(() => cleanup());

test('スレッド読み込み後にレス数 (.t-count-main) と一覧が反映される', async () => {
	const { container } = render(Page);

	// onMount が thread:selected を listen するまで待つ。
	await vi.waitFor(() => expect(listeners.has('thread:selected')).toBe(true));

	// スレッド選択イベントを発火 → currentThreadUrl 設定 → loadCurrentThread。
	await listeners.get('thread:selected')!({
		payload: {
			boardUrl: 'https://jbbs.shitaraba.net/bbs/read.cgi/game/1/',
			key: '100',
			title: 'テストスレ',
		},
	});

	// レス一覧が 3 件描画されるまで待つ。
	await vi.waitFor(() => expect(container.querySelectorAll('.post').length).toBe(3));

	// スレタイ横のレス数が 3 を表示している (0 のまま固着していないこと)。
	const count = container.querySelector('.t-count-main')?.textContent?.trim() ?? '';
	expect(count).toBe('(3)');
});
