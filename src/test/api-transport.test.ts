// ブラウザ transport (pst-server REST) の検証。jsdom では isTauri() が
// false になるので、$lib/api の各関数は HTTP 分岐を通る。global.fetch を
// スタブして「正しいエンドポイント / メソッド / ボディを叩くか」「レスポンス
// の包み方 (例: /api/board の {threads}) を正しく剥がすか」「エラー応答を
// CommandError(code) に正規化するか」を確認する。M3 (transport 抽象化) の
// 回帰テスト。
import { afterEach, expect, test, vi } from 'vitest';
import {
	fetchYpSources,
	fetchChannelInfo,
	listThreads,
	fetchThread,
	postToThread,
	serverRecordStart,
	serverRecordList,
	getConfig,
	peercastPing,
	isTauri,
} from '$lib/api';

// fetch の init を eslint no-undef に引っかかる DOM 型 (RequestInit) に
// 依存せず最小限で表す。
type FetchInit = { method?: string; body?: string; headers?: Record<string, string> };
type FetchCall = { url: string; init?: FetchInit };
let calls: FetchCall[] = [];

function ok(body: unknown): Response {
	return {
		ok: true,
		status: 200,
		text: async () => JSON.stringify(body),
		json: async () => body,
	} as Response;
}

function err(status: number, body: unknown): Response {
	return {
		ok: false,
		status,
		text: async () => JSON.stringify(body),
		json: async () => body,
	} as Response;
}

function stubFetch(responder: (url: string, init?: FetchInit) => Response) {
	calls = [];
	vi.stubGlobal(
		'fetch',
		vi.fn(async (url: string, init?: FetchInit) => {
			calls.push({ url, init });
			return responder(url, init);
		}),
	);
}

afterEach(() => {
	vi.unstubAllGlobals();
});

test('jsdom では isTauri() は false (= ブラウザ transport を通る)', () => {
	expect(isTauri()).toBe(false);
});

test('fetchYpSources は GET /api/yp/all を叩く', async () => {
	stubFetch(() => ok({ entries: [{ id: 'abc', name: 'ch1' }], failures: [] }));
	const r = await fetchYpSources();
	expect(calls[0].url).toBe('/api/yp/all');
	expect(r.entries[0].name).toBe('ch1');
});

test('fetchChannelInfo は GET /api/channel/:id/info を叩く', async () => {
	stubFetch(() => ok({ name: 'ch', bitrate: 500 }));
	const r = await fetchChannelInfo({ host: 'x', port: 7144 }, 'ABCDEF');
	expect(calls[0].url).toBe('/api/channel/ABCDEF/info');
	expect(r.name).toBe('ch');
});

test('listThreads は /api/board の {threads} を取り出す', async () => {
	const board = 'https://jbbs.example/bbs/x/1/';
	stubFetch(() => ok({ kind: 'Shitaraba', threads: [{ key: '1', title: 't', count: 3 }] }));
	const r = await listThreads(board);
	expect(calls[0].url).toBe('/api/board?url=' + encodeURIComponent(board));
	expect(r).toHaveLength(1);
	expect(r[0].title).toBe('t');
});

test('fetchThread は prev を query 化し {posts,state} を tuple に変換する', async () => {
	stubFetch(() =>
		ok({
			kind: 'Shitaraba',
			posts: [{ number: 1 }],
			state: { lastCount: 1, lastByte: 10, lastModified: null },
		}),
	);
	const [posts, state] = await fetchThread('https://x/1/', {
		lastCount: 5,
		lastByte: 99,
		lastModified: 'Mon',
	});
	const u = calls[0].url;
	expect(u.startsWith('/api/thread?')).toBe(true);
	expect(u).toContain('last_count=5');
	expect(u).toContain('last_byte=99');
	expect(u).toContain('last_modified=Mon');
	expect(posts).toHaveLength(1);
	expect(state.lastByte).toBe(10);
});

test('postToThread は POST /api/thread/post に {url,name,mail,body} を送る', async () => {
	stubFetch(() => ok({}));
	await postToThread('https://x/1/', { name: 'n', mail: 'sage', body: 'hi' });
	expect(calls[0].url).toBe('/api/thread/post');
	expect(calls[0].init?.method).toBe('POST');
	expect(JSON.parse(String(calls[0].init?.body))).toEqual({
		url: 'https://x/1/',
		name: 'n',
		mail: 'sage',
		body: 'hi',
	});
});

test('serverRecordStart は POST /api/record/start に {id,name} を送る', async () => {
	stubFetch(() => ok({ channel_id: 'a', channel_name: 'n', path: '/p' }));
	await serverRecordStart('http://ignored-in-browser', 'a', 'n');
	expect(calls[0].url).toBe('/api/record/start');
	expect(JSON.parse(String(calls[0].init?.body))).toEqual({ id: 'a', name: 'n', tip: null });
});

test('serverRecordList は GET /api/record/list の recordings を返す', async () => {
	stubFetch(() => ok({ recordings: [{ channel_id: 'a', channel_name: 'n', path: '/p' }] }));
	const r = await serverRecordList('http://ignored-in-browser');
	expect(calls[0].url).toBe('/api/record/list');
	expect(r).toHaveLength(1);
});

test('エラー応答 (例 404 thread_gone) は CommandError(code) に正規化される', async () => {
	stubFetch(() => err(404, { code: 'thread_gone', message: 'gone' }));
	await expect(fetchThread('https://x/1/')).rejects.toMatchObject({
		name: 'CommandError',
		code: 'thread_gone',
	});
});

test('getConfig はサーバ config を Config 形に適合 (favorites/yp 採用・hub 既定)', async () => {
	stubFetch(() =>
		ok({
			peercast: { host: '192.0.2.9', port: 7144 },
			favorites: { rules: [{ name: 'fav', channel_name: 'X' }] },
			yp: { sources: [{ name: 'SP', url: 'http://x/index.txt' }] },
		}),
	);
	const cfg = await getConfig();
	expect(calls[0].url).toBe('/api/config');
	// サーバ値が採用される
	expect(cfg.peercast.host).toBe('192.0.2.9');
	expect(cfg.favorites?.rules).toHaveLength(1);
	expect(cfg.yp?.sources[0].name).toBe('SP');
	// デスクトップ専用セクションは既定で埋まる (ハブが参照する)
	expect(cfg.bbs).toBeTruthy();
	expect(cfg.player).toBeTruthy();
	expect(cfg.hub?.refresh_sec).toBe(60);
	// ブラウザでは録画可否判定を通すため pst_server_url が truthy
	expect(cfg.hub?.pst_server_url).toBeTruthy();
});

test('peercastPing はブラウザでは no-op (fetch しない)', async () => {
	stubFetch(() => ok({}));
	await expect(peercastPing()).resolves.toBeUndefined();
	expect(calls).toHaveLength(0);
});
