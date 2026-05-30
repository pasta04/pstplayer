// Helpers to open sub-windows (settings / threads).
// See docs/ui-design.md §設定ダイアログ and §スレ一覧ウィンドウ.

import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow } from '@tauri-apps/api/window';

async function openOrFocus(
	label: string,
	url: string,
	options: { title: string; width: number; height: number },
): Promise<WebviewWindow> {
	const existing = await WebviewWindow.getByLabel(label);
	if (existing) {
		await existing.show();
		await existing.setFocus();
		return existing;
	}
	return new WebviewWindow(label, {
		url,
		title: options.title,
		width: options.width,
		height: options.height,
		resizable: true,
		decorations: true,
	});
}

export async function openSettings(): Promise<void> {
	await openOrFocus('settings', '/settings', {
		title: 'PSTPlayer · 設定',
		width: 560,
		height: 540,
	});
}

export async function openThreadList(boardUrl: string): Promise<void> {
	const url = `/threads?board=${encodeURIComponent(boardUrl)}`;
	await openOrFocus('threads', url, {
		title: 'PSTPlayer · スレッド一覧',
		width: 440,
		height: 540,
	});
}

export function thisWindowLabel(): string {
	return getCurrentWindow().label;
}
