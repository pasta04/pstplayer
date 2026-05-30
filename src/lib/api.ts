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
