// Main window geometry persistence.
//
// Stored as PHYSICAL pixels in config.window.{x,y,width,height} so
// multi-monitor / DPI-changing setups stay consistent. Save is
// debounced (500ms) to avoid hammering the disk while dragging or
// resizing.

import { getCurrentWindow, PhysicalPosition, PhysicalSize } from '@tauri-apps/api/window';
import { getConfig, saveWindowGeometry } from './api';

const SAVE_DEBOUNCE_MS = 500;

export async function restoreMainWindowGeometry(): Promise<void> {
	try {
		const cfg = await getConfig();
		const w = cfg?.window;
		if (!w) return;
		const win = getCurrentWindow();
		if (w.width != null && w.height != null && w.width > 0 && w.height > 0) {
			await win.setSize(new PhysicalSize(w.width, w.height));
		}
		if (w.x != null && w.y != null) {
			await win.setPosition(new PhysicalPosition(w.x, w.y));
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
