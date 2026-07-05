// Typed thin wrappers around Tauri commands.
// Keep this file the only place that uses `@tauri-apps/api/core`
// so that the rest of the UI is unaware of the IPC plumbing.

import { invoke } from '@tauri-apps/api/core';

export interface IpcError {
	code: string;
	message: string;
}

export class CommandError extends Error {
	code: string;
	constructor(err: IpcError) {
		super(err.message);
		this.code = err.code;
		this.name = 'CommandError';
	}
}

async function call<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
	try {
		return await invoke<T>(cmd, args);
	} catch (e) {
		if (e && typeof e === 'object' && 'code' in e && 'message' in e) {
			throw new CommandError(e as IpcError);
		}
		throw e;
	}
}

// ── Transport (Tauri IPC / pst-server REST) ──────────────────────────
// 同じ Svelte フロントを (1) Tauri デスクトップ (invoke) と (2) ブラウザ
// (pst-server が配信する Web UI) の両方で動かすための薄い切替層。Tauri
// 実行時は従来どおり invoke、ブラウザ実行時は pst-server の HTTP API を
// 叩く。pst-server のエラー応答は {code, message} で Tauri 側と同じコード
// 体系 (peercast_unreachable / thread_gone 等) なので CommandError に正規化
// すれば UI のエラーハンドリングは両環境で共通になる。

/** Tauri 実行環境か (webview に __TAURI_INTERNALS__ が注入される)。
 * 視聴方式 (libmpv / HLS) や transport の切替に使う。 */
export function isTauri(): boolean {
	return typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;
}

/** pst-server REST のベース URL。pst-server が配信する素のブラウザでは
 * 同一オリジン (空文字 = 相対パス)。dev で別オリジンの pst-server を叩く
 * ときだけ VITE_PST_API_BASE で上書きする。 */
const API_BASE: string = (
	(import.meta.env as Record<string, string | undefined>).VITE_PST_API_BASE ?? ''
).replace(/\/+$/, '');

async function httpRequest<T>(method: string, path: string, body?: unknown): Promise<T> {
	let res: Response;
	try {
		res = await fetch(API_BASE + path, {
			method,
			headers: body === undefined ? undefined : { 'content-type': 'application/json' },
			body: body === undefined ? undefined : JSON.stringify(body),
		});
	} catch (e) {
		// ネットワーク到達不可は CommandError(network) に正規化。
		throw new CommandError({
			code: 'network',
			message: e instanceof Error ? e.message : String(e),
		});
	}
	if (!res.ok) {
		// pst-server は {code, message} の JSON を返す。パースできなければ
		// status から最小の CommandError を組む。
		let code = 'http_error';
		let message = `HTTP ${res.status}`;
		try {
			const j = (await res.json()) as Partial<IpcError>;
			if (typeof j.code === 'string') code = j.code;
			if (typeof j.message === 'string') message = j.message;
		} catch {
			/* non-JSON body */
		}
		throw new CommandError({ code, message });
	}
	if (res.status === 204) return undefined as T;
	const text = await res.text();
	return (text ? JSON.parse(text) : undefined) as T;
}

function httpGet<T>(path: string): Promise<T> {
	return httpRequest<T>('GET', path);
}
function httpPost<T>(path: string, body?: unknown): Promise<T> {
	return httpRequest<T>('POST', path, body);
}
function httpPut<T>(path: string, body?: unknown): Promise<T> {
	return httpRequest<T>('PUT', path, body);
}

/** Tauri なら tauriFn、ブラウザなら httpFn を呼ぶ。レスポンス型は
 * 両 transport で同じ pst-core 型 (同一 serde) なので一致する。 */
function dual<T>(tauriFn: () => Promise<T>, httpFn: () => Promise<T>): Promise<T> {
	return isTauri() ? tauriFn() : httpFn();
}

// ── Types ───────────────────────────────────────────────────────────

export interface BasicAuth {
	user: string;
	pass: string;
}

export interface PeerCastEndpoint {
	host: string;
	port: number;
	auth?: BasicAuth | null;
}

export interface ChannelInfo {
	name: string;
	comment: string;
	desc: string;
	genre: string;
	url: string;
	contentType: string;
	mimeType: string;
	bitrate: number;
	streamType: string;
	streamExt: string;
}

export interface ChannelStatus {
	status: string;
	uptime: number;
	localRelays: number;
	localDirects: number;
	totalRelays: number;
	totalDirects: number;
	isBroadcasting: boolean;
	isRelayFull: boolean;
	isDirectFull: boolean;
	isReceiving: boolean;
}

// ── Commands ────────────────────────────────────────────────────────

export async function ping(): Promise<string> {
	return call<string>('ping');
}

export async function resolveStreamUrl(url: string): Promise<string> {
	return call<string>('resolve_stream_url', { url });
}

export async function endpointForUrl(url: string): Promise<PeerCastEndpoint> {
	return call<PeerCastEndpoint>('endpoint_for_url', { url });
}

/** 起動時の疎通チェック。成功 = PeerCast 本体に応答あり。失敗時は
 * CommandError.code === 'peercast_unreachable'。 */
export async function peercastPing(): Promise<void> {
	return dual(
		() => call<void>('peercast_ping'),
		async () => {
			// ブラウザは pst-server 経由。専用 ping endpoint は無いので no-op
			// (到達性の問題は実データ取得時に surface する)。
		},
	);
}

export interface CliArgs {
	url: string | null;
	channel_name: string | null;
	id: string | null;
	tip: string | null;
	contact: string | null;
	genre: string | null;
	desc: string | null;
	bitrate: number | null;
	content_type: string | null;
	comment: string | null;
	yp_name: string | null;
	yp_url: string | null;
	no_autoplay: boolean;
	no_bbs: boolean;
	minimized: boolean;
	hidden: boolean;
	show_help: boolean;
	show_version: boolean;
	record_on_start: boolean;
}

export async function getCliArgs(): Promise<CliArgs> {
	return call<CliArgs>('get_cli_args');
}

export async function resolveDefaultEndpoint(): Promise<PeerCastEndpoint> {
	return call<PeerCastEndpoint>('resolve_default_endpoint');
}

export async function fetchChannelInfo(
	endpoint: PeerCastEndpoint,
	channelId: string,
): Promise<ChannelInfo> {
	return dual(
		() => call<ChannelInfo>('fetch_channel_info', { endpoint, channelId }),
		() => httpGet<ChannelInfo>(`/api/channel/${encodeURIComponent(channelId)}/info`),
	);
}

export async function fetchChannelStatus(
	endpoint: PeerCastEndpoint,
	channelId: string,
): Promise<ChannelStatus> {
	return dual(
		() => call<ChannelStatus>('fetch_channel_status', { endpoint, channelId }),
		() => httpGet<ChannelStatus>(`/api/channel/${encodeURIComponent(channelId)}/status`),
	);
}

export async function bumpChannel(endpoint: PeerCastEndpoint, channelId: string): Promise<void> {
	return dual(
		() => call<void>('bump_channel', { endpoint, channelId }),
		async () => {
			await httpPost(`/api/channel/${encodeURIComponent(channelId)}/bump`);
		},
	);
}

export async function stopChannel(endpoint: PeerCastEndpoint, channelId: string): Promise<void> {
	return dual(
		() => call<void>('stop_channel', { endpoint, channelId }),
		async () => {
			await httpPost(`/api/channel/${encodeURIComponent(channelId)}/stop`);
		},
	);
}

export async function startChannelPolling(
	endpoint: PeerCastEndpoint,
	channelId: string,
): Promise<void> {
	return call<void>('start_channel_polling', { endpoint, channelId });
}

export async function stopChannelPolling(): Promise<void> {
	return call<void>('stop_channel_polling');
}

export interface YpEntry {
	name: string;
	id: string;
	tip: string;
	contact_url: string;
	genre: string;
	desc: string;
	listeners: number;
	relays: number;
	bitrate: number;
	content_type: string;
	track_artist: string;
	track_album: string;
	track_title: string;
	track_contact: string;
	name_url_encoded: string;
	uptime: string;
	flag_click: string;
	comment: string;
	flag_extra: string;
	/// 取得元 YP の `name` (複数 YP マージ後にバックエンドが付ける)。
	yp_source: string;
}

export interface YpFetchFailure {
	source: string;
	url: string;
	error: string;
}

export interface YpMultiFetchOutcome {
	entries: YpEntry[];
	failures: YpFetchFailure[];
}

export async function fetchYpIndex(overrideUrl?: string): Promise<YpEntry[]> {
	return call<YpEntry[]>('fetch_yp_index', { overrideUrl: overrideUrl ?? null });
}

/// 設定 (`[[yp.sources]]`) に登録された全 YP を並行 fetch して
/// entries + failures をまとめて返す。
export async function fetchYpSources(): Promise<YpMultiFetchOutcome> {
	return dual(
		() => call<YpMultiFetchOutcome>('fetch_yp_sources'),
		() => httpGet<YpMultiFetchOutcome>('/api/yp/all'),
	);
}

export type SpawnViewerOutcome = 'focused' | 'spawned';

/// YP / お気に入りからチャンネルを「別ウィンドウで開く」呼び出し。
/// 既に同じ channel_id の視聴ウィンドウが立ち上がっていれば
/// `'focused'` (前面化のみ)、無ければ `'spawned'` (新規プロセス起動)
/// を返す。
///
/// `options.tip` は YP entry の `tip` (= `host:port`) を渡す。自分の
/// PeerCast がまだ subscribe していないチャンネルでも、tip 経由で
/// 引き込みを発火させて 404 を回避するのに必要。手入力 URL のように
/// tip が分からないときは省略可。
export async function spawnViewer(
	channelId: string,
	options?: { record?: boolean; hidden?: boolean; tip?: string },
): Promise<SpawnViewerOutcome> {
	return call<SpawnViewerOutcome>('spawn_viewer', {
		channelId,
		record: options?.record ?? false,
		hidden: options?.hidden ?? false,
		tip: options?.tip ?? null,
	});
}

/// 現在「視聴中」(= single_instance ロックが生きている) チャンネル ID
/// の配列。ハブ画面で「視聴中」タブを描くのに使う。
export async function listActiveViewers(): Promise<string[]> {
	return call<string[]>('list_active_viewers');
}

/// 指定 channel_id の視聴ウィンドウを閉じる。
export async function closeViewer(channelId: string): Promise<boolean> {
	return call<boolean>('close_viewer', { channelId });
}

/// 全視聴ウィンドウを一括クローズ。閉じた数を返す。
export async function closeAllViewers(): Promise<number> {
	return call<number>('close_all_viewers');
}

/// pst-server (ハブ常駐サーバ) で録画中の 1 件。
export interface ServerRecordingEntry {
	channel_id: string;
	channel_name: string;
	path: string;
}

/// pst-server に「再生せず録画開始」を依頼する (視聴ウィンドウを開かない /
/// 音を出さない)。pst-server が HTTP ストリームを直接ファイルへ保存する。
export async function serverRecordStart(
	serverUrl: string,
	id: string,
	name: string,
	/// 配信元ヒント (host:port)。未リレーのチャンネルは tip 無しだと
	/// PeerCast がソースを見つけられず録画開始に失敗する。
	tip?: string,
): Promise<void> {
	await dual(
		() => call<unknown>('server_record_start', { serverUrl, id, name, tip: tip ?? null }),
		() => httpPost<unknown>('/api/record/start', { id, name, tip: tip ?? null }),
	);
}

/// pst-server の録画を停止 (id 省略で全停止)。
export async function serverRecordStop(serverUrl: string, id?: string): Promise<void> {
	await dual(
		() => call<unknown>('server_record_stop', { serverUrl, id: id ?? null }),
		() => httpPost<unknown>('/api/record/stop', { id: id ?? null }),
	);
}

/// pst-server で録画中のチャンネル一覧。
export async function serverRecordList(serverUrl: string): Promise<ServerRecordingEntry[]> {
	return dual(
		async () => {
			const v = await call<{ recordings: ServerRecordingEntry[] } | null>('server_record_list', {
				serverUrl,
			});
			return v?.recordings ?? [];
		},
		async () => {
			const v = await httpGet<{ recordings: ServerRecordingEntry[] } | null>('/api/record/list');
			return v?.recordings ?? [];
		},
	);
}

// ── BBS ─────────────────────────────────────────────────────────────

export type BoardKind = 'Shitaraba' | 'Ch2Compat';

export interface SubjectEntry {
	key: string;
	title: string;
	count: number;
}

export interface Post {
	number: number;
	name: string;
	mail: string;
	date: string;
	id: string;
	body: string;
	threadTitle: string;
}

export interface FetchState {
	lastModified: string | null;
	lastByte: number;
	lastCount: number;
	/// true ならこの応答は「スレ全体のスナップショット」。呼び出し側は
	/// 手元のレス一覧を置換する (追記すると全レスが二重になる)。
	fullReload?: boolean;
}

export interface PostRequest {
	name: string;
	mail: string;
	body: string;
}

export interface BoardSetting {
	/// 1 スレッドの最大レス数 (したらば BBS_THREAD_STOP / 2ch BBS_RES_MAX)。
	/// 0 なら取得できなかった (呼び出し側で 1000 等にフォールバック)。
	maxRes: number;
	defaultName: string;
	title: string;
}

export async function classifyBoard(url: string): Promise<BoardKind> {
	return call<BoardKind>('classify_board', { url });
}

/// 板の SETTING (最大レス数等) を取得。満レス判定 (自動スレ移動) に使う。
export async function fetchBoardSetting(url: string): Promise<BoardSetting> {
	return dual(
		() => call<BoardSetting>('fetch_board_setting', { url }),
		() => httpGet<BoardSetting>(`/api/board/setting?url=${encodeURIComponent(url)}`),
	);
}

/// 任意の (スレ or 板) URL から、その板のトップ URL を正規化して返す。
export async function boardUrlOf(url: string): Promise<string> {
	return dual(
		() => call<string>('board_url_of', { url }),
		async () =>
			(await httpGet<{ url: string }>(`/api/board/board-url?url=${encodeURIComponent(url)}`)).url,
	);
}

/// 板 URL + スレッド key から、板の流儀に合った canonical なスレッド
/// URL を組み立てる (2ch 互換の `/test/read.cgi/` 等を吸収)。
export async function threadUrlOf(boardUrl: string, key: string): Promise<string> {
	return dual(
		() => call<string>('thread_url_of', { boardUrl, key }),
		async () =>
			(
				await httpGet<{ url: string }>(
					`/api/board/thread-url?url=${encodeURIComponent(boardUrl)}&key=${encodeURIComponent(key)}`,
				)
			).url,
	);
}

export async function listThreads(boardUrl: string): Promise<SubjectEntry[]> {
	return dual(
		() => call<SubjectEntry[]>('list_threads', { boardUrl }),
		async () => {
			const r = await httpGet<{ kind: BoardKind | null; threads: SubjectEntry[] }>(
				`/api/board?url=${encodeURIComponent(boardUrl)}`,
			);
			return r.threads;
		},
	);
}

export async function fetchThread(
	threadUrl: string,
	prev?: FetchState | null,
): Promise<[Post[], FetchState]> {
	return dual(
		() => call<[Post[], FetchState]>('fetch_thread', { threadUrl, prev: prev ?? null }),
		async () => {
			const q = new URLSearchParams({ url: threadUrl });
			if (prev) {
				if (prev.lastCount) q.set('last_count', String(prev.lastCount));
				if (prev.lastByte) q.set('last_byte', String(prev.lastByte));
				if (prev.lastModified) q.set('last_modified', prev.lastModified);
			}
			const r = await httpGet<{ kind: BoardKind | null; posts: Post[]; state: FetchState }>(
				`/api/thread?${q.toString()}`,
			);
			return [r.posts, r.state] as [Post[], FetchState];
		},
	);
}

export async function postToThread(threadUrl: string, req: PostRequest): Promise<void> {
	return dual(
		() => call<void>('post_to_thread', { threadUrl, req }),
		async () => {
			await httpPost('/api/thread/post', {
				url: threadUrl,
				name: req.name,
				mail: req.mail,
				body: req.body,
			});
		},
	);
}

export async function sanitizeHtml(html: string): Promise<string> {
	return call<string>('sanitize_html', { html });
}

// ── Config ──────────────────────────────────────────────────────────

export interface PeerCastConfig {
	host: string;
	port: number;
	authUser: string | null;
	authPass: string | null;
	timeoutSec: number;
	recentHosts: string[];
	ypUrl: string;
}

export interface BbsConfig {
	defaultName: string;
	defaultMail: string;
	autoRefreshSec: number;
	displayMode: 'plain' | 'html';
	submitKey: 'ctrl_enter' | 'shift_enter';
	notifyOnNewPost: boolean;
	autoscroll: boolean;
	autoscrollSpeed: number;
}

export interface PlayerCfg {
	volume: number;
	aspect_mode: string;
	snapshot_dir: string;
	snapshot_format: string;
	snapshot_jpeg_quality: number;
	recording_dir: string;
	recording_ext: string;
	/// 配信切断時に自動再接続するか。既定 false (= 観察モード)。
	/// 詳細は src-tauri/src/player/engine.rs。
	auto_reconnect: boolean;
}

export interface WindowCfg {
	x: number | null;
	y: number | null;
	width: number | null;
	height: number | null;
	bbs_pane_ratio: number | null;
	bbs_pane_position: string | null;
	always_on_top: boolean;
	/// 最小化時にタスクトレイへ格納する (既定 false=タスクバー)。次回起動から反映。
	minimize_to_tray: boolean;
}

export type FavoriteAction = 'show' | 'ignore' | 'block';

export interface FavoriteRule {
	name: string;
	/// マッチパターン (部分一致 / 大文字小文字無視 / `|` 区切り OR)。
	/// match_* でチェックした対象フィールドのどれかに一致すればマッチ。
	/// 空欄なら旧形式 (channel_name 等のフィールド別 AND) にフォールバック。
	pattern: string;
	match_name: boolean;
	match_genre: boolean;
	match_desc: boolean;
	match_comment: boolean;
	/// 旧形式のフィールド別パターン (pattern が空のときだけ使われる)。
	channel_name: string;
	genre: string;
	desc: string;
	comment: string;
	pin_top: boolean;
	auto_record: boolean;
	/// 旧フィールド (互換)。新規は `background` を使う。両方ある時は
	/// `background` 優先。
	color: string;
	background: string;
	text_color: string;
	action: FavoriteAction;
}

export interface FavoritesCfg {
	rules: FavoriteRule[];
}

/// 旧 `color` と新 `background` の救済。
export function effectiveBackground(rule: FavoriteRule | null | undefined): string {
	if (!rule) return '';
	return rule.background?.trim() || rule.color?.trim() || '';
}

export interface YpSource {
	name: string;
	url: string;
	namespace: string;
	show_tab: boolean;
	show_in_all: boolean;
	text_color: string;
	background: string;
}

export interface YpCfg {
	sources: YpSource[];
}

export type HubClickAction = 'none' | 'watch' | 'watch_and_record' | 'open_bbs' | 'open_contact';

export interface HubCfg {
	refresh_sec: number;
	watching_poll_sec: number;
	double_click: HubClickAction;
	middle_click: HubClickAction;
	pst_server_url: string;
}

export interface Config {
	peercast: PeerCastConfig;
	bbs: BbsConfig;
	player: PlayerCfg;
	window?: WindowCfg;
	/** カスタムホットキー (action_id → "Ctrl+Shift+R" 等)。
	 * 未指定の action はフロントのデフォルトを使う。 */
	hotkeys?: Record<string, string>;
	favorites?: FavoritesCfg;
	yp?: YpCfg;
	hub?: HubCfg;
	// other sections exist but are not exposed yet
	[key: string]: unknown;
}

/** お気に入りルールでチャンネル系のオブジェクトを判定する。
 * pst-core::favorites::matches とロジックを揃える。
 * - 新形式 (pattern 非空): チェックした対象フィールドのどれか (OR) に
 *   部分一致すればマッチ。対象が 1 つも無ければマッチしない。
 * - 旧形式 (pattern 空): フィールド別パターンの AND。空欄はワイルドカード。
 * どちらも `|` 区切りで OR (例: "foo|bar|baz")。 */
export function ruleMatches(
	rule: FavoriteRule,
	t: { name: string; genre: string; desc: string; comment: string },
): boolean {
	const part = (needle: string, hay: string) => {
		const n = (needle ?? '').trim();
		if (!n) return true;
		const hayLc = (hay ?? '').toLowerCase();
		return n
			.split('|')
			.map((s) => s.trim())
			.filter(Boolean)
			.some((alt) => hayLc.includes(alt.toLowerCase()));
	};
	const p = (rule.pattern ?? '').trim();
	if (p) {
		const hays: string[] = [];
		if (rule.match_name) hays.push(t.name);
		if (rule.match_genre) hays.push(t.genre);
		if (rule.match_desc) hays.push(t.desc);
		if (rule.match_comment) hays.push(t.comment);
		return hays.some((hay) => part(p, hay));
	}
	return (
		part(rule.channel_name, t.name) &&
		part(rule.genre, t.genre) &&
		part(rule.desc, t.desc) &&
		part(rule.comment, t.comment)
	);
}

export function firstFavoriteMatch(
	rules: FavoriteRule[] | undefined,
	t: { name: string; genre: string; desc: string; comment: string },
): FavoriteRule | null {
	if (!rules) return null;
	for (const r of rules) {
		if (ruleMatches(r, t)) return r;
	}
	return null;
}

/** `YpEntry` を `firstFavoriteMatch` の入力形に変換した上で評価する
 * 薄い helper (hub / yp ページで同じ呼び出しをするため共通化)。 */
export function matchYpEntry(
	rules: FavoriteRule[] | undefined,
	e: { name: string; genre: string; desc: string; comment: string },
): FavoriteRule | null {
	return firstFavoriteMatch(rules, {
		name: e.name,
		genre: e.genre,
		desc: e.desc,
		comment: e.comment,
	});
}

/** pst-server `/api/config` のうちブラウザのハブが使う部分。サーバ用
 * スキーマ (peercast/server/log/recording/favorites/yp) のサブセット。 */
interface ServerConfigResponse {
	peercast?: { host?: string; port?: number; auth_user?: string | null; auth_pass?: string | null };
	favorites?: FavoritesCfg;
	yp?: YpCfg;
	bbs?: { default_name?: string; default_mail?: string };
}

/** pst-server のサーバ用 config を、ブラウザのハブが期待する Config 形へ
 * 適合させる。favorites / yp / peercast はサーバ値を使い、hub / bbs /
 * player などデスクトップ専用セクションは既定値で埋める。ブラウザでは
 * pst-server 自身が同一オリジンなので hub.pst_server_url に origin を入れ、
 * 録画操作 (pstServerUrl 空ならブロック) を有効化する。 */
function browserConfig(s: ServerConfigResponse): Config {
	return {
		peercast: {
			host: s.peercast?.host ?? 'localhost',
			port: s.peercast?.port ?? 7144,
			authUser: s.peercast?.auth_user ?? null,
			authPass: s.peercast?.auth_pass ?? null,
			timeoutSec: 10,
			recentHosts: [],
			ypUrl: '',
		},
		bbs: {
			defaultName: s.bbs?.default_name ?? '',
			defaultMail: s.bbs?.default_mail ?? 'sage',
			autoRefreshSec: 5,
			displayMode: 'plain',
			submitKey: 'ctrl_enter',
			notifyOnNewPost: false,
			autoscroll: true,
			autoscrollSpeed: 1,
		},
		player: {
			volume: 80,
			aspect_mode: '',
			snapshot_dir: '',
			snapshot_format: 'png',
			snapshot_jpeg_quality: 90,
			recording_dir: '',
			recording_ext: '',
			auto_reconnect: false,
		},
		favorites: s.favorites ?? { rules: [] },
		yp: s.yp ?? { sources: [] },
		hub: {
			refresh_sec: 60,
			watching_poll_sec: 5,
			double_click: 'watch',
			middle_click: 'open_bbs',
			pst_server_url: typeof window !== 'undefined' ? window.location.origin : '',
		},
	};
}

export async function getConfig(): Promise<Config> {
	return dual(
		() => call<Config>('get_config'),
		async () => browserConfig(await httpGet<ServerConfigResponse>('/api/config')),
	);
}

export async function setConfig(config: Config): Promise<void> {
	return call<void>('set_config', { config });
}

export async function configFilePath(): Promise<string> {
	return dual(
		() => call<string>('config_file_path'),
		async () => (await httpGet<{ path: string }>('/api/config/path')).path,
	);
}

/** pst-server の生 config (サーバ用スキーマ)。ブラウザの設定画面は
 * これを取得し、編集対象のセクション (peercast / yp / favorites) だけ
 * 差し替えて PUT する。未知のセクション (server / log / recording 等)
 * を Record のまま保持して往復させることで消さない。 */
export async function getServerConfigRaw(): Promise<Record<string, unknown>> {
	return httpGet<Record<string, unknown>>('/api/config');
}

export async function putServerConfigRaw(cfg: Record<string, unknown>): Promise<void> {
	await httpPut('/api/config', cfg);
}

export async function saveWindowGeometry(
	x: number,
	y: number,
	width: number,
	height: number,
): Promise<void> {
	return call<void>('save_window_geometry', { x, y, width, height });
}

export async function pushRecentHost(host: string, port: number): Promise<void> {
	return call<void>('push_recent_host', { host, port });
}

// ── Player (libmpv) ─────────────────────────────────────────────────

export interface PlayerStatus {
	fps: number | null;
	width: number | null;
	height: number | null;
	timePos: number | null;
	paused: boolean | null;
}

export async function playerLoad(url: string): Promise<void> {
	return call<void>('player_load', { url });
}

export async function playerAttach(windowLabel: string): Promise<void> {
	return call<void>('player_attach', { windowLabel });
}

/// libmpv 描画用の子ウィンドウを、プレイヤー領域の **物理ピクセル** 矩形
/// (親ウィンドウのクライアント座標) に合わせる。`.player-canvas` の
/// getBoundingClientRect() × devicePixelRatio を渡す。Windows 以外は no-op。
export async function playerSetVideoRect(
	x: number,
	y: number,
	width: number,
	height: number,
): Promise<void> {
	return call<void>('player_set_video_rect', { x, y, width, height });
}

export async function playerStop(): Promise<void> {
	return call<void>('player_stop');
}

export async function playerSetPause(pause: boolean): Promise<void> {
	return call<void>('player_set_pause', { pause });
}

export async function playerSetVolume(percent: number): Promise<void> {
	return call<void>('player_set_volume', { percent });
}

export async function playerSetMute(mute: boolean): Promise<void> {
	return call<void>('player_set_mute', { mute });
}

export async function playerStatus(): Promise<PlayerStatus> {
	return call<PlayerStatus>('player_status');
}

export async function playerSnapshot(channelName?: string): Promise<string> {
	return call<string>('player_snapshot', { channelName: channelName ?? '' });
}

export async function snapshotTargetDir(): Promise<string> {
	return call<string>('snapshot_target_dir');
}

export async function recordingTargetDir(): Promise<string> {
	return call<string>('recording_target_dir');
}

export async function playerSetAspect(aspect: number): Promise<void> {
	return call<void>('player_set_aspect', { aspect });
}

/// 自動再接続の ON/OFF を即時反映する。設定ダイアログの保存ハンドラ
/// から呼ぶ。詳細は src-tauri/src/player/engine.rs を参照。
export async function playerSetAutoReconnect(enabled: boolean): Promise<void> {
	return call<void>('player_set_auto_reconnect', { enabled });
}
