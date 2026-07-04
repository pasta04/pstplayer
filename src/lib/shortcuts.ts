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
	/** 切断してウィンドウを閉じる (PCRPlayer の Alt+X 相当)。 */
	disconnectAndClose: () => void;
	pasteUrl: () => void;
	openSettings: () => void;
	openThreadList: () => void;
	focusSearch: () => void;
	reloadThread: () => void;
	reloadThreadFull: () => void;
	snapshot: () => void;
	toggleFullscreen: () => void;
	/** index 1-9; caller maps to a concrete percent. */
	setSizePreset: (idx: number) => void;
	/** index 1-7; caller maps to a concrete aspect-ratio override. */
	setAspectPreset: (idx: number) => void;
}

// ── ホットキーバインディング ──────────────────────────────────
//
// 文字列形式: `Ctrl+Shift+R` / `Alt+Enter` / `T` / `F2` 等。
// `Cmd` / `Meta` / `Option` は Ctrl / Alt に正規化。
// 大文字小文字は無視 (`t` と `T` は同じ)。

export interface ParsedBinding {
	ctrl: boolean;
	shift: boolean;
	alt: boolean;
	key: string; // lower-case 名前 ('r', 'f2', 'enter', ',')
}

export function parseBinding(s: string): ParsedBinding | null {
	const raw = s.trim();
	if (!raw) return null;
	const parts = raw.split('+').map((p) => p.trim());
	if (parts.length === 0) return null;
	let ctrl = false;
	let shift = false;
	let alt = false;
	let key = '';
	for (const p of parts) {
		const lower = p.toLowerCase();
		if (lower === 'ctrl' || lower === 'control' || lower === 'cmd' || lower === 'meta') {
			ctrl = true;
		} else if (lower === 'shift') {
			shift = true;
		} else if (lower === 'alt' || lower === 'option') {
			alt = true;
		} else if (lower.length > 0) {
			key = lower;
		}
	}
	if (!key) return null;
	return { ctrl, shift, alt, key };
}

export function matchesBinding(e: KeyboardEvent, b: ParsedBinding): boolean {
	const ctrl = e.ctrlKey || e.metaKey;
	return (
		ctrl === b.ctrl && e.shiftKey === b.shift && e.altKey === b.alt && e.key.toLowerCase() === b.key
	);
}

export function formatBinding(b: ParsedBinding): string {
	const parts: string[] = [];
	if (b.ctrl) parts.push('Ctrl');
	if (b.shift) parts.push('Shift');
	if (b.alt) parts.push('Alt');
	parts.push(displayKey(b.key));
	return parts.join('+');
}

function displayKey(key: string): string {
	// 1 文字なら大文字化 (T, R 等)、それ以外は initial cap (F2, Enter, F11)。
	if (key.length === 1) return key.toUpperCase();
	if (key.startsWith('f') && /^f\d+$/.test(key)) return key.toUpperCase();
	return key.charAt(0).toUpperCase() + key.slice(1);
}

/**
 * KeyboardEvent から binding 形式の文字列を作る。
 * 編集 UI のキャプチャに使う。
 */
export function bindingFromEvent(e: KeyboardEvent): string | null {
	const k = e.key;
	// 単独の Modifier キーは無視 (Ctrl だけ押した状態を確定させない)。
	if (k === 'Control' || k === 'Shift' || k === 'Alt' || k === 'Meta') return null;
	const b: ParsedBinding = {
		ctrl: e.ctrlKey || e.metaKey,
		shift: e.shiftKey,
		alt: e.altKey,
		key: k.toLowerCase(),
	};
	return formatBinding(b);
}

// ── アクション定義 ──────────────────────────────────────────────

export interface HotkeyDef {
	/** TOML config key */
	id: string;
	/** 設定画面に出すラベル */
	label: string;
	/** デフォルトバインディング (空文字列にすると「割当無し」) */
	defaultBinding: string;
	/** ShortcutActions のメソッド名 */
	action: keyof ShortcutActions;
}

export const HOTKEY_DEFS: HotkeyDef[] = [
	{
		id: 'toggle_always_on_top',
		label: '常に最前面',
		defaultBinding: 'T',
		action: 'toggleAlwaysOnTop',
	},
	{ id: 'toggle_frame', label: 'ウィンドウ枠 ON/OFF', defaultBinding: 'Z', action: 'toggleFrame' },
	{
		id: 'toggle_title_bar',
		label: 'タイトル帯 ON/OFF',
		defaultBinding: 'X',
		action: 'toggleTitleBar',
	},
	{
		id: 'toggle_status_bar',
		label: 'ステータスバー ON/OFF',
		defaultBinding: 'B',
		action: 'toggleStatusBar',
	},
	{
		id: 'toggle_bbs_pane',
		label: 'BBS ペイン ON/OFF',
		defaultBinding: 'C',
		action: 'toggleBbsPane',
	},
	{
		id: 'toggle_fullscreen',
		label: '全画面切替',
		defaultBinding: 'F11',
		action: 'toggleFullscreen',
	},
	{ id: 'bump', label: '再接続 (Bump)', defaultBinding: 'F5', action: 'bump' },
	{ id: 'stop', label: '切断 (Stop)', defaultBinding: 'Ctrl+F5', action: 'stop' },
	{
		id: 'disconnect_and_close',
		label: '切断してウィンドウを閉じる',
		defaultBinding: 'Alt+X',
		action: 'disconnectAndClose',
	},
	{
		id: 'paste_url',
		label: 'クリップボードから URL',
		defaultBinding: 'Ctrl+V',
		action: 'pasteUrl',
	},
	{ id: 'open_settings', label: '設定を開く', defaultBinding: 'Ctrl+,', action: 'openSettings' },
	{
		id: 'open_thread_list',
		label: 'スレ一覧を開く',
		defaultBinding: 'Ctrl+L',
		action: 'openThreadList',
	},
	{
		id: 'focus_search',
		label: 'スレ内検索にフォーカス',
		defaultBinding: 'Ctrl+F',
		action: 'focusSearch',
	},
	{
		id: 'reload_thread',
		label: 'スレ差分更新',
		defaultBinding: 'Ctrl+R',
		action: 'reloadThread',
	},
	{
		id: 'reload_thread_full',
		label: 'スレ完全再取得',
		defaultBinding: 'Ctrl+Shift+R',
		action: 'reloadThreadFull',
	},
	{ id: 'snapshot', label: 'スナップショット', defaultBinding: 'F2', action: 'snapshot' },
];

/**
 * カスタマイズ不可な「予約済み」キー一覧。`installShortcuts` 内で
 * `actions` の前に発火するため、ユーザが普通のホットキーに割り当てても
 * 黙って奪われる (= silent failure)。`detectConflicts` でこれを表示する
 * ために事前に signature 化しておく。
 *
 * - `Ctrl+1`..`Ctrl+9`: サイズプリセット
 * - `Alt+1`..`Alt+7`: アスペクト比
 * - `Alt+Enter`: 全画面切替の代替バインディング
 * - `Escape`: 全画面解除 (input 内でも有効)
 */
const RESERVED_BINDINGS: { label: string; sig: string }[] = (() => {
	const out: { label: string; sig: string }[] = [];
	for (let i = 1; i <= 9; i++) out.push({ label: `Ctrl+${i} (サイズ)`, sig: `c|${i}` });
	for (let i = 1; i <= 7; i++) out.push({ label: `Alt+${i} (アスペクト比)`, sig: `a|${i}` });
	out.push({ label: 'Alt+Enter (全画面)', sig: 'a|enter' });
	out.push({ label: 'Esc (全画面解除)', sig: '|escape' });
	return out;
})();

function bindingSig(p: ParsedBinding): string {
	return `${p.ctrl ? 'c' : ''}${p.shift ? 's' : ''}${p.alt ? 'a' : ''}|${p.key}`;
}

/**
 * バインディングの衝突 (同じキー組合せに 2 つ以上のアクション) を検査。
 * 返り値: id ごとの衝突相手 id 一覧。
 *
 * カスタマイズ不可な「予約済み」キー (Ctrl+1〜9 / Alt+1〜7 / Alt+Enter /
 * Esc) との衝突も含めて検出する。これらと被るとユーザのカスタム binding
 * が silent に効かなくなる (予約済みが先に発火) ため、設定 UI で警告
 * できるようにする。
 */
export function detectConflicts(custom: Record<string, string>): Record<string, string[]> {
	const bySig = new Map<string, string[]>();
	// 予約済みキーを先に登録 (HOTKEY_DEFS の id と衝突しないよう "reserved:"
	// プレフィクスを付ける)。
	for (const r of RESERVED_BINDINGS) {
		bySig.set(r.sig, [`reserved:${r.label}`]);
	}
	const allBindings = new Map<string, string>();
	for (const def of HOTKEY_DEFS) {
		allBindings.set(def.id, (custom[def.id] ?? def.defaultBinding).trim());
	}
	for (const [id, b] of allBindings) {
		if (!b) continue;
		const parsed = parseBinding(b);
		if (!parsed) continue;
		const sig = bindingSig(parsed);
		const list = bySig.get(sig) ?? [];
		list.push(id);
		bySig.set(sig, list);
	}
	const result: Record<string, string[]> = {};
	for (const ids of bySig.values()) {
		if (ids.length < 2) continue;
		for (const id of ids) {
			if (id.startsWith('reserved:')) continue;
			result[id] = ids.filter((x) => x !== id);
		}
	}
	return result;
}

/**
 * Install the global keyboard listener and return an unbind function.
 * Skips events that originate from text inputs/textareas so they don't
 * interfere with the write-box.
 *
 * `customs` で各 action.id にカスタム binding を上書き指定可。空文字列
 * を渡すと「割当無し」として無効化。
 */
export function installShortcuts(
	actions: ShortcutActions,
	customs: Record<string, string> = {},
): () => void {
	// 各アクションに有効な ParsedBinding を作っておく (毎 keydown で
	// parse し直さなくて済む)。
	type Slot = { action: () => void; binding: ParsedBinding };
	const slots: Slot[] = [];
	for (const def of HOTKEY_DEFS) {
		const raw = (customs[def.id] ?? def.defaultBinding).trim();
		if (!raw) continue; // 割当無し
		const parsed = parseBinding(raw);
		if (!parsed) continue;
		const fn = actions[def.action] as () => void;
		if (typeof fn === 'function') {
			slots.push({ action: fn, binding: parsed });
		}
	}

	const isFromInput = (e: KeyboardEvent) =>
		e.target instanceof HTMLInputElement ||
		e.target instanceof HTMLTextAreaElement ||
		(e.target instanceof HTMLElement && e.target.isContentEditable);

	const handler = (e: KeyboardEvent) => {
		// Always allow Esc to exit fullscreen, even from inputs.
		if (e.key === 'Escape') {
			const win = getCurrentWindow();
			win.isFullscreen().then((fs) => {
				if (fs) win.setFullscreen(false);
			});
			return;
		}

		if (isFromInput(e)) return;

		const ctrl = e.ctrlKey || e.metaKey;

		// 固定: Ctrl+1..9 = サイズプリセット (PCRPlayer 互換、カスタム不可)
		if (ctrl && !e.altKey && !e.shiftKey && /^[1-9]$/.test(e.key)) {
			actions.setSizePreset(Number(e.key));
			e.preventDefault();
			return;
		}
		// 固定: Alt+1..7 = アスペクト比
		if (!ctrl && e.altKey && !e.shiftKey && /^[1-7]$/.test(e.key)) {
			actions.setAspectPreset(Number(e.key));
			e.preventDefault();
			return;
		}
		// 固定: Alt+Enter = 全画面切替 (代替バインディング)
		if (e.altKey && e.key === 'Enter') {
			actions.toggleFullscreen();
			e.preventDefault();
			return;
		}

		// データ駆動のアクション群
		for (const slot of slots) {
			if (matchesBinding(e, slot.binding)) {
				slot.action();
				// Ctrl+V だけは「テキスト欄にフォーカスが移った後の貼り付け」
				// にも回せるよう preventDefault を控える。それ以外は WebView の
				// 既定動作 (ページ reload など) を抑止する。
				if (slot.binding.ctrl && slot.binding.key === 'v') return;
				e.preventDefault();
				return;
			}
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

/** 現在のウィンドウを閉じる (Alt+X の「切断して閉じる」で使用)。
 * close() は capability (core:window:allow-close) が無いと reject して
 * ウィンドウが閉じずに残る事故が起きた。許可は default.json に追加済み
 * だが、万一 close() が失敗した場合も destroy() で確実に閉じる。 */
export async function closeWindow(): Promise<void> {
	const win = getCurrentWindow();
	try {
		await win.close();
	} catch (e) {
		console.error('window close failed, falling back to destroy', e);
		await win.destroy();
	}
}
