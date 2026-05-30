// Keyboard shortcut handling for the main window.
// Mirrors docs/shortcuts.md.

import { getCurrentWindow } from '@tauri-apps/api/window';

export interface ShortcutActions {
	toggleBbsPane: () => void;
	toggleStatusBar: () => void;
	toggleTitleBar: () => void;
	toggleFrame: () => void;
	toggleAlwaysOnTop: () => void;
	bump: () => void;
	stop: () => void;
	pasteUrl: () => void;
	openSettings: () => void;
	openThreadList: () => void;
}

/**
 * Install the global keyboard listener and return an unbind function.
 * Skips events that originate from text inputs/textareas so they don't
 * interfere with the write-box.
 */
export function installShortcuts(actions: ShortcutActions): () => void {
	const win = getCurrentWindow();

	const isFromInput = (e: KeyboardEvent) =>
		e.target instanceof HTMLInputElement ||
		e.target instanceof HTMLTextAreaElement ||
		(e.target instanceof HTMLElement && e.target.isContentEditable);

	async function toggleFullscreen() {
		const cur = await win.isFullscreen();
		await win.setFullscreen(!cur);
	}

	const handler = (e: KeyboardEvent) => {
		// Always allow Esc to exit fullscreen, even from inputs.
		if (e.key === 'Escape') {
			win.isFullscreen().then((fs) => {
				if (fs) win.setFullscreen(false);
			});
			return;
		}

		if (isFromInput(e)) return;

		const ctrl = e.ctrlKey || e.metaKey;

		// Single-letter toggles (PCRPlayer compatible)
		if (!ctrl && !e.altKey && !e.shiftKey) {
			switch (e.key.toLowerCase()) {
				case 't':
					actions.toggleAlwaysOnTop();
					e.preventDefault();
					return;
				case 'z':
					actions.toggleFrame();
					e.preventDefault();
					return;
				case 'x':
					actions.toggleTitleBar();
					e.preventDefault();
					return;
				case 'b':
					actions.toggleStatusBar();
					e.preventDefault();
					return;
				case 'c':
					actions.toggleBbsPane();
					e.preventDefault();
					return;
			}
		}

		// Player + navigation
		if (e.key === 'Enter' || e.key === 'F11' || (e.altKey && e.key === 'Enter')) {
			toggleFullscreen();
			e.preventDefault();
			return;
		}
		if (e.key === 'F5' && !ctrl) {
			actions.bump();
			e.preventDefault();
			return;
		}
		if (e.key === 'F5' && ctrl) {
			actions.stop();
			e.preventDefault();
			return;
		}
		if (ctrl && e.key === 'v') {
			actions.pasteUrl();
			// don't preventDefault — let the browser paste also fire if the box has focus.
			return;
		}
		if (ctrl && e.key === 'l') {
			actions.openThreadList();
			e.preventDefault();
			return;
		}
		if (ctrl && e.key === ',') {
			actions.openSettings();
			e.preventDefault();
			return;
		}
	};

	window.addEventListener('keydown', handler);
	return () => window.removeEventListener('keydown', handler);
}

/** Tauri window helpers used by the UI. */
export async function setAlwaysOnTop(on: boolean): Promise<void> {
	await getCurrentWindow().setAlwaysOnTop(on);
}

export async function setDecorations(on: boolean): Promise<void> {
	await getCurrentWindow().setDecorations(on);
}
