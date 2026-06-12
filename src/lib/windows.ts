// Helpers to open sub-windows (settings / threads).
// See docs/ui-design.md §設定ダイアログ and §スレ一覧ウィンドウ.

import { WebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow } from '@tauri-apps/api/window';

/// 引数なしで開くサブウィンドウ向け。既存があれば show + focus、
/// 無ければ新規作成。URL は label 単位で 1 つしか持たない前提なので
/// re-navigate が要らない (= settings / yp / channel-info 等)。
async function openOrFocus(
	label: string,
	url: string,
	options: { title: string; width: number; height: number },
): Promise<WebviewWindow> {
	const existing = await WebviewWindow.getByLabel(label);
	if (existing) {
		try {
			await existing.show();
			await existing.setFocus();
			return existing;
		} catch {
			// race: getByLabel と show の間にウィンドウが破棄された可能性。
			// 新規作成にフォールスルー。
		}
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

/// URL クエリ引数で開くサブウィンドウ向け。既存ウィンドウがあっても
/// URL を再ナビゲートするため、一度 close してから新規作成する。
/// Tauri 2 には `WebviewWindow.navigate()` が無いのでこのパターンが
/// 唯一の手段。
async function reopenWithUrl(
	label: string,
	url: string,
	options: { title: string; width: number; height: number },
): Promise<void> {
	const existing = await WebviewWindow.getByLabel(label);
	if (existing) {
		try {
			await existing.close();
		} catch {
			// 既に閉じている / race。気にしない。
		}
		// Tauri は close 完了を待たずに次の new WebviewWindow を許すと
		// 「同じ label がまだ生きている」エラーを返すことがある。
		// onCloseRequested を待つ手もあるが、軽い待機で実用十分。
		await new Promise((r) => setTimeout(r, 50));
	}
	new WebviewWindow(label, {
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
	// スレ一覧は boardUrl が URL クエリ引数なので、別チャンネルに切替えた
	// 後に既存ウィンドウを show するだけだと古い板が見え続ける。reopenWithUrl
	// で必ず新規作成する。
	await reopenWithUrl('threads', url, {
		title: 'PSTPlayer · スレッド一覧',
		width: 440,
		height: 540,
	});
}

export async function openYpList(): Promise<void> {
	await openOrFocus('yp', '/yp', {
		title: 'PSTPlayer · YP チャンネル一覧',
		width: 760,
		height: 540,
	});
}

export function thisWindowLabel(): string {
	return getCurrentWindow().label;
}
