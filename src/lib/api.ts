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

export interface Config {
	peercast: PeerCastConfig;
	bbs: BbsConfig;
	player: PlayerCfg;
	window?: WindowCfg;
	// other sections exist but are not exposed yet
	[key: string]: unknown;
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

export async function playerSetAspect(aspect: number): Promise<void> {
	return call<void>('player_set_aspect', { aspect });
}
