// Main window geometry persistence.
//
// Stored as PHYSICAL pixels in config.window.{x,y,width,height} so
// multi-monitor / DPI-changing setups stay consistent. Save is
// debounced (500ms) to avoid hammering the disk while dragging or
// resizing.

import {
	availableMonitors,
	currentMonitor,
	getCurrentWindow,
	PhysicalPosition,
	PhysicalSize,
} from '@tauri-apps/api/window';
import { getConfig, saveWindowGeometry } from './api';

const SAVE_DEBOUNCE_MS = 500;

/// 保存された (x, y, width, height) が、現在の物理モニタ範囲に少しでも
/// 重なっているかを判定。1 ピクセルでも見えていれば「画面内」とみなす。
/// マルチモニタ構成変更後 (= 前回は外部モニタにいたが今は本体だけ)、
/// 保存した位置が完全に off-screen になっていることがあるため、その時は
/// 復元をスキップして Tauri の既定位置 (本体モニタ中央) に任せる。
function rectsOverlap(
	a: { x: number; y: number; width: number; height: number },
	b: { x: number; y: number; width: number; height: number },
): boolean {
	return a.x < b.x + b.width && a.x + a.width > b.x && a.y < b.y + b.height && a.y + a.height > b.y;
}

async function isOnVisibleMonitor(
	x: number,
	y: number,
	width: number,
	height: number,
): Promise<boolean> {
	try {
		const monitors = await availableMonitors();
		if (monitors.length === 0) {
			// monitor 情報が取れない (Wayland 等) なら復元を許可。
			return true;
		}
		const wantRect = { x, y, width, height };
		return monitors.some((m) => {
			const mp = m.position;
			const ms = m.size;
			return rectsOverlap(wantRect, {
				x: mp.x,
				y: mp.y,
				width: ms.width,
				height: ms.height,
			});
		});
	} catch {
		return true;
	}
}

export async function restoreMainWindowGeometry(): Promise<void> {
	try {
		const cfg = await getConfig();
		const w = cfg?.window;
		if (!w) return;
		const win = getCurrentWindow();
		const haveSize = w.width != null && w.height != null && w.width > 0 && w.height > 0;
		const havePos = w.x != null && w.y != null;
		if (haveSize && havePos) {
			// 画面範囲チェック (off-screen 救済)
			const visible = await isOnVisibleMonitor(w.x!, w.y!, w.width!, w.height!);
			if (!visible) {
				// 完全に画面外 → サイズだけ復元 + 位置は Tauri 既定に任せる。
				// (= 主画面の中央寄り)
				await win.setSize(new PhysicalSize(w.width!, w.height!));
				// 位置の保存を消す。次回からは新しい妥当な位置がここに入る。
				const cur = await currentMonitor();
				if (cur) {
					const cx = cur.position.x + Math.max(0, Math.floor((cur.size.width - w.width!) / 2));
					const cy = cur.position.y + Math.max(0, Math.floor((cur.size.height - w.height!) / 2));
					await win.setPosition(new PhysicalPosition(cx, cy));
				}
				return;
			}
			await win.setSize(new PhysicalSize(w.width!, w.height!));
			await win.setPosition(new PhysicalPosition(w.x!, w.y!));
		} else if (haveSize) {
			await win.setSize(new PhysicalSize(w.width!, w.height!));
		}
	} catch {
		/* first launch or no perms — fall back to Tauri defaults */
	}
}

/**
 * Subscribe to move/resize and persist geometry on quiet. Returns an
 * unbind function that cancels all listeners and the pending timer.
 */
export async function watchMainWindowGeometry(): Promise<() => void> {
	const win = getCurrentWindow();
	let timer: ReturnType<typeof setTimeout> | null = null;

	async function flush() {
		timer = null;
		try {
			const pos = await win.outerPosition();
			const size = await win.outerSize();
			if (size.width <= 0 || size.height <= 0) return;
			await saveWindowGeometry(pos.x, pos.y, size.width, size.height);
		} catch {
			/* ignore — best effort */
		}
	}

	function schedule() {
		if (timer) clearTimeout(timer);
		timer = setTimeout(flush, SAVE_DEBOUNCE_MS);
	}

	const unMoved = await win.onMoved(schedule);
	const unResized = await win.onResized(schedule);

	return () => {
		if (timer) clearTimeout(timer);
		unMoved();
		unResized();
	};
}
