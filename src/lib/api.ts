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

export async function ping(): Promise<string> {
	return call<string>('ping');
}

export async function resolveStreamUrl(url: string): Promise<string> {
	return call<string>('resolve_stream_url', { url });
}
