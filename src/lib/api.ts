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
	return call<void>('peercast_ping');
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
	return call<ChannelInfo>('fetch_channel_info', { endpoint, channelId });
}

export async function fetchChannelStatus(
	endpoint: PeerCastEndpoint,
	channelId: string,
): Promise<ChannelStatus> {
	return call<ChannelStatus>('fetch_channel_status', { endpoint, channelId });
}

export async function bumpChannel(endpoint: PeerCastEndpoint, channelId: string): Promise<void> {
	return call<void>('bump_channel', { endpoint, channelId });
}

export async function stopChannel(endpoint: PeerCastEndpoint, channelId: string): Promise<void> {
	return call<void>('stop_channel', { endpoint, channelId });
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
	return call<YpMultiFetchOutcome>('fetch_yp_sources');
}

export type SpawnViewerOutcome = 'focused' | 'spawned';

/// YP / お気に入りからチャンネルを「別ウィンドウで開く」呼び出し。
/// 既に同じ channel_id の視聴ウィンドウが立ち上がっていれば
/// `'focused'` (前面化のみ)、無ければ `'spawned'` (新規プロセス起動)
/// を返す。
export async function spawnViewer(
	channelId: string,
	options?: { record?: boolean },
): Promise<SpawnViewerOutcome> {
	return call<SpawnViewerOutcome>('spawn_viewer', {
		channelId,
		record: options?.record ?? false,
	});
}

/// 現在「視聴中」(= single_instance ロックが生きている) チャンネル ID
/// の配列。ハブ画面で「視聴中」タブを描くのに使う。
export async function listActiveViewers(): Promise<string[]> {
	return call<string[]>('list_active_viewers');
}

/// 現在「録画中」の channel_id 配列。視聴中の中でさらに状態問い合わせを
/// 投げ、`state` IPC で true 応答が返ったものだけを含む。ハブ画面の
/// 「録画中」タブ表示用。
export async function listRecordingViewers(): Promise<string[]> {
	return call<string[]>('list_recording_viewers');
}

/// 指定 channel_id の視聴ウィンドウを閉じる。
export async function closeViewer(channelId: string): Promise<boolean> {
	return call<boolean>('close_viewer', { channelId });
}

/// 全視聴ウィンドウを一括クローズ。閉じた数を返す。
export async function closeAllViewers(): Promise<number> {
	return call<number>('close_all_viewers');
}

/// 指定 channel_id の視聴ウィンドウに録画停止を要求。
export async function stopViewerRecording(channelId: string): Promise<boolean> {
	return call<boolean>('stop_viewer_recording', { channelId });
}

/// 指定 channel_id の視聴ウィンドウに録画開始を要求。
export async function startViewerRecording(channelId: string): Promise<boolean> {
	return call<boolean>('start_viewer_recording', { channelId });
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
}

export interface PostRequest {
	name: string;
	mail: string;
	body: string;
}

export async function classifyBoard(url: string): Promise<BoardKind> {
	return call<BoardKind>('classify_board', { url });
}

export async function listThreads(boardUrl: string): Promise<SubjectEntry[]> {
	return call<SubjectEntry[]>('list_threads', { boardUrl });
}

export async function fetchThread(
	threadUrl: string,
	prev?: FetchState | null,
): Promise<[Post[], FetchState]> {
	return call<[Post[], FetchState]>('fetch_thread', { threadUrl, prev: prev ?? null });
}

export async function postToThread(threadUrl: string, req: PostRequest): Promise<void> {
	return call<void>('post_to_thread', { threadUrl, req });
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
}

export interface PlayerCfg {
	volume: number;
	aspect_mode: string;
	snapshot_dir: string;
	snapshot_format: string;
	snapshot_jpeg_quality: number;
	recording_dir: string;
	recording_ext: string;
}

export interface WindowCfg {
	x: number | null;
	y: number | null;
	width: number | null;
	height: number | null;
	bbs_pane_ratio: number | null;
	bbs_pane_position: string | null;
	always_on_top: boolean;
}

export type FavoriteAction = 'show' | 'ignore' | 'block';

export interface FavoriteRule {
	name: string;
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
 * 全フィールド空欄ならワイルドカード、複数指定は AND。
 * `|` 区切りで OR (例: "foo|bar|baz")。pst-core::favorites::matches と
 * ロジックを揃える。 */
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

export async function getConfig(): Promise<Config> {
	return call<Config>('get_config');
}

export async function setConfig(config: Config): Promise<void> {
	return call<void>('set_config', { config });
}

export async function configFilePath(): Promise<string> {
	return call<string>('config_file_path');
}

export interface HistoryEntry {
	url: string;
	channelName: string;
	lastOpenedAt: number;
}

export async function pushHistory(url: string, channelName: string): Promise<void> {
	return call<void>('push_history', { url, channelName });
}

export async function getHistory(): Promise<HistoryEntry[]> {
	return call<HistoryEntry[]>('get_history');
}

export async function clearHistory(): Promise<void> {
	return call<void>('clear_history');
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

export async function playerRecordStart(channelName?: string): Promise<string> {
	return call<string>('player_record_start', { channelName: channelName ?? '' });
}

export async function playerRecordStop(): Promise<void> {
	return call<void>('player_record_stop');
}

export async function playerRecordPath(): Promise<string | null> {
	return call<string | null>('player_record_path');
}

export async function recordingTargetDir(): Promise<string> {
	return call<string>('recording_target_dir');
}

export async function playerSetAspect(aspect: number): Promise<void> {
	return call<void>('player_set_aspect', { aspect });
}
