<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import {
		CommandError,
		bumpChannel,
		endpointForUrl,
		fetchChannelInfo,
		fetchChannelStatus,
		fetchThread,
		getCliArgs,
		getConfig,
		listThreads,
		playerAttach,
		playerLoad,
		playerSetAspect,
		playerSetVolume,
		playerSnapshot,
		playerStatus,
		playerStop,
		postToThread,
		pushHistory,
		resolveStreamUrl,
		sanitizeHtml,
		stopChannel,
		type PlayerStatus,
		type ChannelInfo,
		type ChannelStatus,
		type FetchState,
		type PeerCastEndpoint,
		type Post,
		type SubjectEntry,
	} from '$lib/api';
	import { formatUptime, linkifySanitized, renderBodyHtml, renderIdHtml } from '$lib/format';
	import { openSettings, openThreadList } from '$lib/windows';
	import { installShortcuts, setAlwaysOnTop, setDecorations } from '$lib/shortcuts';
	import { notify } from '$lib/notifications';
	import { initTheme } from '$lib/theme';
	import { restoreMainWindowGeometry, watchMainWindowGeometry } from '$lib/window-state';

	// ── State ────────────────────────────────────────────────────────

	let pasteUrl = $state('');
	let busy = $state(false);
	let lastError = $state<string | null>(null);

	// ── Display toggles (T/Z/X/B/C shortcuts + 表示 menu) ────────────
	let showBbsPane = $state(true);
	let showStatusBar = $state(true);
	let showTitleBar = $state(true); // titlebar text visibility (decorations stay)
	let showFrame = $state(true); // window decorations
	let alwaysOnTop = $state(false);

	let streamUrl = $state<string | null>(null);
	let endpoint = $state<PeerCastEndpoint | null>(null);
	let channelId = $state<string | null>(null);
	let channelInfo = $state<ChannelInfo | null>(null);
	let channelStatus = $state<ChannelStatus | null>(null);

	let threadList = $state<SubjectEntry[]>([]);
	let currentThreadUrl = $state<string | null>(null);
	let posts = $state<Post[]>([]);
	let fetchState = $state<FetchState | null>(null);
	let threadLoading = $state(false);

	let writeName = $state('');
	let writeMail = $state('sage');
	let writeBody = $state('');
	let writeSending = $state(false);

	// Anchor / ID popup
	type Popup = { posts: Post[]; label: string; x: number; y: number } | null;
	let popup = $state<Popup>(null);

	// In-thread filter (Ctrl+F to focus, empty string = show all)
	let filter = $state('');
	let filterInput: HTMLInputElement | null = $state(null);

	// Display mode (plain text vs HTML rendering). Sourced from config
	// on mount and cached. Defaults to plain.
	let displayMode = $state<'plain' | 'html'>('plain');
	// Submit key for the write box. Loaded from config on mount.
	let submitKey = $state<'ctrl_enter' | 'shift_enter'>('ctrl_enter');
	// OS notification on new posts. Hidden setting (TOML only), default
	// off because frequent posts make it noisy across multiple windows.
	let notifyOnNewPost = $state(false);
	// Auto-scroll the post list to the bottom when new posts arrive,
	// unless the user has manually scrolled up.
	let autoscroll = $state(true);
	let postsEl: HTMLDivElement | null = $state(null);
	// Cache of sanitised HTML per post number to avoid re-fetching on
	// every render.
	let sanitizedCache = $state<Map<number, string>>(new Map());

	// Hovered ID for highlight, or null. Mouse over an ID link in the
	// post header highlights every post that shares the same ID.
	let hoveredId = $state<string | null>(null);

	// Polled from the libmpv engine (player_status). null until first
	// successful sample.
	let playerStat = $state<PlayerStatus | null>(null);

	// Right-click context menu over the player area.
	let ctxMenu = $state<{ x: number; y: number } | null>(null);

	// Volume (0-100). Wheel over the player area changes it.
	let volume = $state(80);

	// Seconds until the next BBS auto-refresh tick (5s cycle).
	const REFRESH_SEC = 5;
	let refreshCountdown = $state(REFRESH_SEC);

	// Polling handles
	let infoTimer: ReturnType<typeof setInterval> | null = null;
	let threadTimer: ReturnType<typeof setInterval> | null = null;
	let playerTimer: ReturnType<typeof setInterval> | null = null;
	let countdownTimer: ReturnType<typeof setInterval> | null = null;
	let threadSelectedUnlisten: UnlistenFn | null = null;
	let configSavedUnlisten: UnlistenFn | null = null;

	let shortcutsUnlisten: (() => void) | null = null;
	let themeUnlisten: (() => void) | null = null;
	let windowGeomUnlisten: (() => void) | null = null;

	onMount(async () => {
		themeUnlisten = initTheme();

		// Hand the main Tauri window to libmpv so it renders into our surface
		// (`wid` property). Best-effort: on Wayland this is unsupported and
		// the engine just stays detached, which is fine for headless / dev.
		playerAttach('main').catch((e) => {
			console.warn('player_attach failed (libmpv overlay disabled)', e);
		});

		// Restore last main window position/size, then start watching.
		await restoreMainWindowGeometry();
		windowGeomUnlisten = await watchMainWindowGeometry();

		// Load BBS display mode + submit key from config (best-effort).
		await reloadBbsPrefs();

		// Re-read the same prefs whenever the settings window saves.
		configSavedUnlisten = await listen('config:saved', () => {
			reloadBbsPrefs();
		});

		threadSelectedUnlisten = await listen<{
			boardUrl: string;
			key: string;
			title: string;
		}>('thread:selected', async (e) => {
			const base = e.payload.boardUrl.replace(/\/+$/, '');
			currentThreadUrl = `${base}/${e.payload.key}/`;
			fetchState = null;
			posts = [];
			await loadCurrentThread(true);
		});

		// Honour CLI args (positional URL → auto-play unless --no-autoplay).
		try {
			const cli = await getCliArgs();
			if (cli.url && !cli.no_autoplay) {
				pasteUrl = cli.url;
				await onPaste();
			}
		} catch {
			/* CLI parsing is best-effort */
		}

		shortcutsUnlisten = installShortcuts({
			toggleBbsPane: () => (showBbsPane = !showBbsPane),
			toggleStatusBar: () => (showStatusBar = !showStatusBar),
			toggleTitleBar: () => (showTitleBar = !showTitleBar),
			toggleFrame: async () => {
				showFrame = !showFrame;
				await setDecorations(showFrame);
			},
			toggleAlwaysOnTop: async () => {
				alwaysOnTop = !alwaysOnTop;
				await setAlwaysOnTop(alwaysOnTop);
			},
			bump: onBump,
			stop: onStop,
			pasteUrl: async () => {
				try {
					const text = await navigator.clipboard.readText();
					if (text.trim().startsWith('http')) {
						pasteUrl = text.trim();
						await onPaste();
					}
				} catch {
					/* clipboard permission denied — ignore */
				}
			},
			openSettings: onOpenSettings,
			openThreadList: onOpenThreadList,
			focusSearch: () => filterInput?.focus(),
			reloadThread: () => {
				if (currentThreadUrl && !threadLoading) loadCurrentThread(false);
			},
			reloadThreadFull: () => {
				if (currentThreadUrl && !threadLoading) {
					fetchState = null;
					posts = [];
					sanitizedCache = new Map();
					loadCurrentThread(true);
				}
			},
			snapshot: doSnapshot,
			setSizePreset: applySizePreset,
			setAspectPreset: applyAspectPreset,
		});
	});

	onDestroy(() => {
		if (infoTimer) clearInterval(infoTimer);
		if (threadTimer) clearInterval(threadTimer);
		if (playerTimer) clearInterval(playerTimer);
		if (countdownTimer) clearInterval(countdownTimer);
		threadSelectedUnlisten?.();
		configSavedUnlisten?.();
		shortcutsUnlisten?.();
		themeUnlisten?.();
		windowGeomUnlisten?.();
	});

	async function reloadBbsPrefs() {
		try {
			const cfg = await getConfig();
			displayMode = cfg?.bbs?.displayMode === 'html' ? 'html' : 'plain';
			submitKey = cfg?.bbs?.submitKey === 'shift_enter' ? 'shift_enter' : 'ctrl_enter';
			notifyOnNewPost = cfg?.bbs?.notifyOnNewPost === true;
			autoscroll = cfg?.bbs?.autoscroll !== false;
		} catch {
			/* defaults */
		}
	}

	// ── Post-list auto scroll ────────────────────────────────────────
	//
	// Spec (docs/ui-design.md §147): デフォルト ON。手動スクロール時は
	// 一時停止 = ユーザーが末尾付近にいない時は追従しない。
	const NEAR_BOTTOM_PX = 24;

	function isNearBottom(el: HTMLElement | null): boolean {
		if (!el) return false;
		return el.scrollTop + el.clientHeight >= el.scrollHeight - NEAR_BOTTOM_PX;
	}

	function scrollPostsToBottom() {
		if (postsEl) postsEl.scrollTop = postsEl.scrollHeight;
	}

	// ── Derived ──────────────────────────────────────────────────────

	const visiblePosts = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		if (!q) return posts;
		return posts.filter(
			(p) =>
				p.body.toLowerCase().includes(q) ||
				p.name.toLowerCase().includes(q) ||
				p.id.toLowerCase().includes(q) ||
				String(p.number) === q,
		);
	});

	const statusLine = $derived.by(() => {
		if (!channelInfo) return null;
		const name = channelInfo.name || '(unnamed)';
		const br = channelInfo.bitrate ? `${channelInfo.bitrate} kbps` : '-';
		const up = channelStatus ? formatUptime(channelStatus.uptime) : '-';
		const ldir = channelStatus ? `L:${channelStatus.localDirects}` : '';
		const lrel = channelStatus ? `R:${channelStatus.localRelays}` : '';
		const fps = playerStat?.fps && playerStat.fps > 0 ? `${playerStat.fps.toFixed(1)}fps` : '';
		const size =
			playerStat?.width && playerStat?.height ? `${playerStat.width}×${playerStat.height}` : '';
		return { name, br, up, ldir, lrel, fps, size };
	});

	// ── URL paste / load channel ─────────────────────────────────────

	async function onPaste() {
		busy = true;
		lastError = null;
		try {
			const url = pasteUrl.trim();
			if (!url) return;
			streamUrl = await resolveStreamUrl(url);
			endpoint = await endpointForUrl(url);
			channelId = extractChannelId(url);

			// Hand the resolved stream URL to libmpv. Errors here shouldn't
			// abort the BBS / channel-info wiring below.
			try {
				await playerLoad(streamUrl);
			} catch (e) {
				console.warn('player_load failed', e);
			}

			if (channelId && endpoint) {
				await reloadInfoAndBbs();
				startPolling();
				// Record after channelInfo fetch so we have a name.
				pushHistory(url, channelInfo?.name || '').catch(() => undefined);
			}
		} catch (e) {
			lastError = errorMessage(e);
		} finally {
			busy = false;
		}
	}

	function extractChannelId(url: string): string | null {
		const m = url.match(/\/(?:pls|stream)\/([0-9A-Fa-f]{32})/);
		return m ? m[1].toLowerCase() : null;
	}

	async function reloadInfoAndBbs() {
		if (!endpoint || !channelId) return;
		try {
			channelInfo = await fetchChannelInfo(endpoint, channelId);
			channelStatus = await fetchChannelStatus(endpoint, channelId);
		} catch (e) {
			console.warn('channel info fetch failed', e);
		}
		if (channelInfo?.url) {
			await tryLoadBoard(channelInfo.url);
		}
	}

	// Drop any trailing browser-only suffix (e.g. `/l30`, `/501-1000`) and
	// guarantee the URL ends with `/{key}/`. This keeps Range-based
	// incremental fetch and write.cgi POST happy regardless of how the
	// user (or PeerCast contact URL) spelled the link.
	function normalizeThreadUrl(url: string, key: string): string {
		const idx = url.lastIndexOf(`/${key}`);
		if (idx < 0) return url;
		return `${url.slice(0, idx)}/${key}/`;
	}

	async function tryLoadBoard(contactUrl: string) {
		try {
			threadList = await listThreads(contactUrl);
			// Auto-pick the contact URL if it already names a thread.
			// 末尾のサフィックス (l30, 501-1000 等) があっても許容し、
			// canonical /{key}/ 形に正規化してから保存する。
			const m = contactUrl.match(/\/(\d+)(?:\/[^/]*)?\/?$/);
			if (m) {
				const key = m[1];
				currentThreadUrl = normalizeThreadUrl(contactUrl, key);
				await loadCurrentThread(true);
			} else if (threadList.length > 0) {
				// Use shitaraba/2ch URL builder from contact URL + key.
				currentThreadUrl = null;
				posts = [];
			}
		} catch (e) {
			console.warn('board load failed', e);
		}
	}

	async function loadCurrentThread(forceReset: boolean) {
		if (!currentThreadUrl) return;
		threadLoading = true;
		try {
			const prev = forceReset ? null : fetchState;
			const [newPosts, newState] = await fetchThread(currentThreadUrl, prev);
			// Snapshot whether the user was anchored to the bottom *before*
			// we mutate `posts`, so reactive re-render extends the
			// scrollable area without losing the anchor.
			const wasAtBottom = isNearBottom(postsEl);
			let appendedNew = false;
			if (forceReset || !fetchState) {
				posts = newPosts;
				if (forceReset) sanitizedCache = new Map();
			} else if (newPosts.length > 0) {
				posts = [...posts, ...newPosts];
				appendedNew = true;
				if (notifyOnNewPost) {
					const preview = newPosts[0].body.replace(/\s+/g, ' ').slice(0, 80);
					const title = `新着 ${newPosts.length} 件 / ${posts[0]?.threadTitle || ''}`;
					notify(title, preview);
				}
			}
			fetchState = newState;
			if (appendedNew && autoscroll && wasAtBottom) {
				await tick();
				scrollPostsToBottom();
			}

			// Pre-fetch sanitised HTML for the new posts in HTML mode.
			if (displayMode === 'html') {
				for (const p of newPosts) {
					if (sanitizedCache.has(p.number)) continue;
					sanitizeHtml(p.body)
						.then((sanitized) => {
							const next = new Map(sanitizedCache);
							next.set(p.number, linkifySanitized(sanitized));
							sanitizedCache = next;
						})
						.catch(() => undefined);
				}
			}
		} catch (e) {
			lastError = errorMessage(e);
		} finally {
			threadLoading = false;
		}
	}

	function startPolling() {
		if (infoTimer) clearInterval(infoTimer);
		if (threadTimer) clearInterval(threadTimer);
		if (playerTimer) clearInterval(playerTimer);
		infoTimer = setInterval(() => {
			if (endpoint && channelId) {
				fetchChannelStatus(endpoint, channelId).then(
					(s) => (channelStatus = s),
					() => undefined,
				);
			}
		}, 5_000);
		threadTimer = setInterval(() => {
			if (currentThreadUrl && !threadLoading) loadCurrentThread(false);
			refreshCountdown = REFRESH_SEC;
		}, REFRESH_SEC * 1_000);
		countdownTimer = setInterval(() => {
			if (refreshCountdown > 0) refreshCountdown -= 1;
		}, 1_000);
		playerTimer = setInterval(() => {
			playerStatus().then(
				(s) => (playerStat = s),
				() => undefined,
			);
		}, 1_000);
	}

	// ── Channel actions ──────────────────────────────────────────────

	async function onBump() {
		if (!endpoint || !channelId) return;
		try {
			await bumpChannel(endpoint, channelId);
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	async function onStop() {
		if (!endpoint || !channelId) return;
		if (!confirm('チャンネルを切断します。よろしいですか?')) return;
		try {
			await stopChannel(endpoint, channelId);
			await playerStop().catch((e) => console.warn('player_stop failed', e));
			streamUrl = null;
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	// ── BBS actions ──────────────────────────────────────────────────

	async function pickThread(entry: SubjectEntry) {
		if (!channelInfo?.url) return;
		// Build a thread URL relative to the board URL.
		const base = channelInfo.url.replace(/\/+$/, '');
		// shitaraba: ${base}/bbs/read.cgi/${cat}/${board}/${key}/  but
		// we don't have cat/board parsed here, so try /${key}/ first.
		// In practice, contact URL points to the thread directly,
		// so this branch only runs for board-top contact URLs.
		const url = `${base}/${entry.key}/`;
		currentThreadUrl = url;
		fetchState = null;
		posts = [];
		await loadCurrentThread(true);
	}

	async function onSubmit() {
		if (!currentThreadUrl) return;
		if (!writeBody.trim()) return;
		const ok = confirm(`書き込みを送信します:\n\n${writeBody}\n\nよろしいですか?`);
		if (!ok) return;
		writeSending = true;
		try {
			await postToThread(currentThreadUrl, { name: writeName, mail: writeMail, body: writeBody });
			writeBody = '';
			// Refresh immediately
			await loadCurrentThread(false);
		} catch (e) {
			lastError = errorMessage(e);
		} finally {
			writeSending = false;
		}
	}

	function onPlayerContextMenu(e: MouseEvent) {
		e.preventDefault();
		ctxMenu = { x: e.clientX, y: e.clientY };
	}

	function onPlayerWheel(e: WheelEvent) {
		// Wheel up increases volume, wheel down decreases.
		e.preventDefault();
		const step = 5;
		const delta = e.deltaY < 0 ? step : -step;
		const next = Math.max(0, Math.min(150, volume + delta));
		if (next === volume) return;
		volume = next;
		playerSetVolume(volume).catch(() => undefined);
	}

	function closeCtxMenu() {
		ctxMenu = null;
	}

	async function ctxCopyChannelUrl() {
		closeCtxMenu();
		const url = pasteUrl.trim();
		if (!url) return;
		try {
			await navigator.clipboard.writeText(url);
		} catch {
			/* permission denied */
		}
	}

	async function ctxOpenContactUrl() {
		closeCtxMenu();
		if (channelInfo?.url) await openExternal(channelInfo.url);
	}

	async function ctxToggleFullscreen() {
		closeCtxMenu();
		const { getCurrentWindow } = await import('@tauri-apps/api/window');
		const w = getCurrentWindow();
		await w.setFullscreen(!(await w.isFullscreen()));
	}

	async function ctxToggleAlwaysOnTop() {
		closeCtxMenu();
		alwaysOnTop = !alwaysOnTop;
		await setAlwaysOnTop(alwaysOnTop);
	}

	async function doSnapshot() {
		try {
			const path = await playerSnapshot(channelInfo?.name);
			notify('スナップショット保存', path);
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	const SIZE_PERCENTS = [50, 75, 100, 125, 150, 175, 200, 250, 300];
	const ASPECT_PRESETS: { ratio: number; label: string }[] = [
		{ ratio: 0, label: '自動' },
		{ ratio: 16 / 9, label: '16:9' },
		{ ratio: 4 / 3, label: '4:3' },
		{ ratio: 16 / 10, label: '16:10' },
		{ ratio: 5 / 4, label: '5:4' },
		{ ratio: 2.35, label: '2.35:1' },
		{ ratio: -1, label: 'ストレッチ' },
	];

	async function applySizePreset(idx: number) {
		const pct = SIZE_PERCENTS[idx - 1];
		if (!pct) return;
		const baseW = playerStat?.width ?? 1280;
		const baseH = playerStat?.height ?? 720;
		const videoW = Math.round((baseW * pct) / 100);
		const videoH = Math.round((baseH * pct) / 100);
		// Add up the auxiliary UI strips: thread-title (24) + write-box
		// (~24, can grow) + status-bar (24). BBS pane only adds width
		// when it's currently shown.
		const totalW = videoW + (showBbsPane ? 320 : 0);
		const totalH = videoH + 24 + 24 + 24;
		try {
			const { getCurrentWindow, LogicalSize } = await import('@tauri-apps/api/window');
			await getCurrentWindow().setSize(new LogicalSize(totalW, totalH));
		} catch {
			/* tauri unavailable (dev preview) */
		}
	}

	function applyAspectPreset(idx: number) {
		const p = ASPECT_PRESETS[idx - 1];
		if (!p) return;
		playerSetAspect(p.ratio).catch(() => undefined);
	}

	async function onOpenSettings() {
		try {
			await openSettings();
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	async function onOpenThreadList() {
		if (!channelInfo?.url) return;
		try {
			await openThreadList(channelInfo.url);
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	function onWriteKey(e: KeyboardEvent) {
		// Submit key is configurable (bbs.submit_key). Plain Enter is
		// always a newline so that multi-line posts work naturally.
		if (e.key !== 'Enter') return;
		const isCtrl = e.ctrlKey || e.metaKey;
		const isShift = e.shiftKey && !isCtrl;
		const match =
			submitKey === 'shift_enter' ? isShift && !e.altKey : isCtrl && !e.shiftKey && !e.altKey;
		if (match) {
			e.preventDefault();
			onSubmit();
		}
	}

	function onPostsHover(e: MouseEvent) {
		const t = e.target;
		if (!(t instanceof HTMLElement)) return;
		const id = t.dataset.idLink;
		hoveredId = id ?? null;
	}

	function onPostsLeave() {
		hoveredId = null;
	}

	function onPostsFocusIn(e: FocusEvent) {
		// Mirror onPostsHover for keyboard users.
		const t = e.target;
		if (!(t instanceof HTMLElement)) return;
		const id = t.dataset.idLink;
		hoveredId = id ?? null;
	}

	function onPostsKeyDown(e: KeyboardEvent) {
		// Treat Enter/Space on a post-number "button" the same as click.
		if (e.key !== 'Enter' && e.key !== ' ') return;
		const t = e.target;
		if (!(t instanceof HTMLElement) || !t.dataset.postNum) return;
		e.preventDefault();
		insertQuote(Number(t.dataset.postNum));
	}

	function onPostsClick(e: MouseEvent) {
		const t = e.target;
		if (!(t instanceof HTMLElement)) return;

		// Body 内の URL ([format.ts] が `<a class="external">` で出力) は
		// WebView 内で target="_blank" が機能しないので、ここで横取りして
		// OS の既定ブラウザに渡す。
		const a = t.closest('a.external');
		if (a instanceof HTMLAnchorElement && a.href) {
			e.preventDefault();
			openExternal(a.href);
			return;
		}

		const from = t.dataset.anchorFrom;
		const to = t.dataset.anchorTo;
		const id = t.dataset.idLink;
		const num = t.dataset.postNum;
		if (num) {
			insertQuote(Number(num));
			e.preventDefault();
			return;
		}
		if (from) {
			const fromN = Number(from);
			const toN = to ? Number(to) : fromN;
			const matched = posts.filter((p) => p.number >= fromN && p.number <= toN);
			showPopup(matched, `>>${from}${to ? `-${to}` : ''}`, e);
			e.preventDefault();
			return;
		}
		if (id) {
			const matched = posts.filter((p) => p.id === id);
			showPopup(matched, `ID:${id} (${matched.length})`, e);
			e.preventDefault();
		}
	}

	async function openExternal(url: string) {
		try {
			const { openUrl } = await import('@tauri-apps/plugin-opener');
			await openUrl(url);
		} catch (err) {
			console.warn('failed to open external URL', err);
		}
	}

	function insertQuote(n: number) {
		const prefix = writeBody.length > 0 && !writeBody.endsWith('\n') ? '\n' : '';
		writeBody = `${writeBody}${prefix}>>${n}\n`;
	}

	function showPopup(matched: Post[], label: string, e: MouseEvent) {
		if (matched.length === 0) return;
		popup = { posts: matched, label, x: e.clientX, y: e.clientY };
	}

	function closePopup() {
		popup = null;
	}

	function onPopupKey(e: KeyboardEvent) {
		if (e.key === 'Escape') closePopup();
	}

	function errorMessage(e: unknown): string {
		if (e instanceof CommandError) return `${e.code}: ${e.message}`;
		return String(e);
	}
</script>

<svelte:head>
	<title>PSTPlayer</title>
</svelte:head>

<div
	class="app"
	class:hide-bbs={!showBbsPane}
	class:hide-status={!showStatusBar}
	class:hide-title={!showTitleBar}
>
	<!-- Player + BBS panes -->
	<div class="panes">
		<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
		<div
			class="player"
			oncontextmenu={onPlayerContextMenu}
			onwheel={onPlayerWheel}
			ondblclick={ctxToggleFullscreen}
			role="presentation"
		>
			{#if streamUrl}
				<!-- libmpv が wid 経由でこの領域に直接描画する。
				     DOM 上は空のままで OK (動画は native overlay)。 -->
				<div class="player-canvas" aria-label="再生中"></div>
			{:else}
				<div class="player-empty">
					<form
						class="url-form"
						onsubmit={(e) => {
							e.preventDefault();
							onPaste();
						}}
					>
						<label for="paste">PeerCast URL を貼り付け</label>
						<div class="row">
							<input
								id="paste"
								type="text"
								placeholder="http://localhost:7144/pls/0123…"
								bind:value={pasteUrl}
								autocomplete="off"
								spellcheck="false"
								disabled={busy}
							/>
							<button type="submit" disabled={busy || !pasteUrl.trim()}>
								{busy ? '…' : 'Open'}
							</button>
						</div>
					</form>
				</div>
			{/if}
		</div>

		<div class="bbs">
			{#if posts.length === 0 && !currentThreadUrl}
				<div class="bbs-empty">
					{#if threadList.length > 0}
						<div class="hint">スレッド一覧:</div>
						<ul class="thread-list">
							{#each threadList as t (t.key)}
								<li>
									<button class="thread-item" onclick={() => pickThread(t)}>
										<span class="t-title">{t.title}</span>
										<span class="t-count">({t.count})</span>
									</button>
								</li>
							{/each}
						</ul>
					{:else if channelInfo?.url}
						<div class="hint">対応する掲示板が見つかりません。</div>
						<div class="muted">contact: <a href={channelInfo.url}>{channelInfo.url}</a></div>
					{:else}
						<div class="hint muted">URL を貼り付けてチャンネルを開いてください。</div>
					{/if}
				</div>
			{:else}
				<div class="filter-bar">
					<input
						bind:this={filterInput}
						bind:value={filter}
						type="search"
						placeholder="スレ内検索 (本文/名前/ID/番号)"
					/>
					{#if filter}
						<span class="filter-stat">{visiblePosts.length} / {posts.length}</span>
						<button class="filter-clear" onclick={() => (filter = '')}>×</button>
					{/if}
				</div>
				<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
				<!-- svelte-ignore a11y_mouse_events_have_key_events -->
				<div
					class="posts"
					role="list"
					bind:this={postsEl}
					onclick={onPostsClick}
					onkeydown={onPostsKeyDown}
					onmouseover={onPostsHover}
					onmouseleave={onPostsLeave}
					onfocusin={onPostsFocusIn}
				>
					{#each visiblePosts as p (p.number)}
						<div class="post" role="listitem" class:highlight-id={hoveredId && p.id === hoveredId}>
							<div class="head">
								<span
									class="num"
									role="button"
									tabindex="-1"
									data-post-num={p.number}
									title="クリックで &gt;&gt;{p.number} を書き込み欄に挿入">{p.number}</span
								>
								<span class="name">{p.name}</span>
								{#if p.mail}<span class="mail">[{p.mail}]</span>{/if}
								<span class="date">{p.date}</span>
								{#if p.id}<span class="id">{@html renderIdHtml(p.id)}</span>{/if}
							</div>
							<div class="body">
								{#if displayMode === 'html' && sanitizedCache.has(p.number)}
									{@html sanitizedCache.get(p.number) ?? ''}
								{:else}
									{@html renderBodyHtml(p.body)}
								{/if}
							</div>
						</div>
					{/each}
				</div>
				{#if threadLoading}
					<div class="muted small">更新中…</div>
				{/if}
			{/if}
		</div>
	</div>

	<!-- Thread title bar (オレンジ) -->
	<div class="thread-bar">
		<button
			class="thread-bar-button"
			title="スレッド一覧を開く"
			onclick={onOpenThreadList}
			disabled={!channelInfo?.url}
		>
			{#if currentThreadUrl}
				<span class="t-title-main">
					{posts[0]?.threadTitle || currentThreadUrl}
				</span>
				<span class="t-count-main">({posts.length})</span>
			{:else}
				<span class="muted">— スレッド未選択 —</span>
			{/if}
			<span class="t-grow"></span>
			{#if currentThreadUrl}
				<span class="t-refresh" title="次の自動更新までの秒">↻ {refreshCountdown}s</span>
			{/if}
			<span class="t-list">≡</span>
		</button>
	</div>

	<!-- Write box (黒) -->
	<div class="write-box">
		<textarea
			placeholder={currentThreadUrl
				? `ここに書き込む  (${submitKey === 'shift_enter' ? 'Shift' : 'Ctrl/Cmd'}+Enter で送信)`
				: '書き込み欄'}
			bind:value={writeBody}
			onkeydown={onWriteKey}
			disabled={!currentThreadUrl || writeSending}
			rows={Math.max(1, writeBody.split('\n').length)}
		></textarea>
		<button
			class="send"
			onclick={onSubmit}
			disabled={!currentThreadUrl || writeSending || !writeBody.trim()}
		>
			✎
		</button>
	</div>

	<!-- Right-click context menu -->
	{#if ctxMenu}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_click_events_have_key_events -->
		<div
			class="ctx-backdrop"
			role="presentation"
			onclick={closeCtxMenu}
			oncontextmenu={(e) => {
				e.preventDefault();
				closeCtxMenu();
			}}
		>
			<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
			<div
				class="ctx"
				role="menu"
				tabindex="-1"
				style="left: {Math.min(ctxMenu.x, window.innerWidth - 220)}px; top: {Math.min(
					ctxMenu.y,
					window.innerHeight - 240,
				)}px"
				onclick={(e) => e.stopPropagation()}
			>
				<button class="ctx-item" onclick={onBump} disabled={!channelId}> ↻ 再接続 (Bump) </button>
				<button class="ctx-item" onclick={onStop} disabled={!channelId}> ■ 切断 (Stop) </button>
				<div class="ctx-sep"></div>
				<button class="ctx-item" onclick={ctxToggleFullscreen}>⛶ 全画面切替</button>
				<button class="ctx-item" onclick={ctxToggleAlwaysOnTop}>
					{alwaysOnTop ? '✓' : '　'} 常に最前面
				</button>
				<button class="ctx-item" onclick={ctxOpenContactUrl} disabled={!channelInfo?.url}>
					🔗 コンタクト URL を開く
				</button>
				<button class="ctx-item" onclick={ctxCopyChannelUrl}>📋 チャンネル URL をコピー</button>
				<div class="ctx-sep"></div>
				<button
					class="ctx-item"
					onclick={() => {
						closeCtxMenu();
						doSnapshot();
					}}
				>
					📷 スナップショット (F2)
				</button>
				<button class="ctx-item" onclick={onOpenSettings}>⚙ 設定...</button>
				<button class="ctx-item" onclick={onOpenThreadList} disabled={!channelInfo?.url}>
					≡ スレ一覧を開く
				</button>
			</div>
		</div>
	{/if}

	<!-- Anchor / ID popup overlay -->
	{#if popup}
		<!-- svelte-ignore a11y_no_noninteractive_tabindex a11y_no_noninteractive_element_interactions -->
		<div
			class="popup-backdrop"
			role="dialog"
			tabindex="-1"
			onclick={closePopup}
			onkeydown={onPopupKey}
		>
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
			<div
				class="popup"
				role="document"
				tabindex="0"
				onclick={(e) => e.stopPropagation()}
				onkeydown={onPopupKey}
				style="left: {Math.min(popup.x, window.innerWidth - 380)}px; top: {Math.min(
					popup.y,
					window.innerHeight - 320,
				)}px"
			>
				<div class="popup-head">
					<span>{popup.label} — {popup.posts.length} 件</span>
					<button class="popup-close" onclick={closePopup}>×</button>
				</div>
				<div class="posts in-popup" role="list" onclick={onPostsClick} onkeydown={onPostsKeyDown}>
					{#each popup.posts as p (p.number)}
						<div class="post" role="listitem">
							<div class="head">
								<span
									class="num"
									role="button"
									tabindex="-1"
									data-post-num={p.number}
									title="クリックで &gt;&gt;{p.number} を書き込み欄に挿入">{p.number}</span
								>
								<span class="name">{p.name}</span>
								<span class="date">{p.date}</span>
								{#if p.id}<span class="id">{@html renderIdHtml(p.id)}</span>{/if}
							</div>
							<div class="body">
								{#if displayMode === 'html' && sanitizedCache.has(p.number)}
									{@html sanitizedCache.get(p.number) ?? ''}
								{:else}
									{@html renderBodyHtml(p.body)}
								{/if}
							</div>
						</div>
					{/each}
				</div>
			</div>
		</div>
	{/if}

	<!-- Status bar (緑) -->
	<div class="status-bar">
		{#if statusLine}
			<span class="s-name">{statusLine.name}</span>
			<span class="s-info">
				{statusLine.br}
				{#if statusLine.fps}({statusLine.fps}){/if}
				{statusLine.ldir}
				{statusLine.lrel}
			</span>
			{#if statusLine.size}<span class="s-size">{statusLine.size}</span>{/if}
			<span class="s-up">{statusLine.up}</span>
			<span class="s-vol" title="マウスホイールで音量調整">♪ {volume}</span>
			<span class="s-actions">
				<button onclick={onBump} title="再接続 (Bump)">↻</button>
				<button onclick={onStop} title="切断 (Stop)">■</button>
				<button onclick={onOpenSettings} title="設定">⚙</button>
			</span>
		{:else}
			<span class="muted">未接続</span>
		{/if}
		{#if lastError}
			<span class="err">⚠ {lastError}</span>
		{/if}
	</div>
</div>

<style>
	:global(html, body) {
		height: 100%;
		margin: 0;
		font-family:
			-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, 'Noto Sans CJK JP',
			sans-serif;
		background: var(--bg);
		color: var(--fg);
	}

	.app {
		display: grid;
		grid-template-rows: 1fr 24px auto 24px;
		height: 100vh;
		overflow: hidden;
	}

	/* Display toggles (shortcuts.md §1 表示要素 ON/OFF) */
	.app.hide-status .status-bar {
		display: none;
	}
	.app.hide-title .thread-bar {
		display: none;
	}
	.app.hide-bbs .bbs {
		display: none;
	}
	.app.hide-bbs .panes {
		grid-template-columns: 1fr;
	}

	.panes {
		display: grid;
		grid-template-columns: 1fr 320px;
		min-height: 0; /* allow children to shrink */
	}

	.player {
		background: #000;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: 0;
	}

	.player-empty {
		color: #9aa0a6;
		text-align: center;
		padding: 2rem;
	}

	/* libmpv 描画用の透明な場所取り。動画は native overlay として
	   この div の矩形に重ねて描かれる。背景は親 .player の黒。 */
	.player-canvas {
		flex: 1 1 auto;
		align-self: stretch;
	}

	.url-form label {
		display: block;
		margin-bottom: 0.5rem;
		font-size: 0.9rem;
		color: var(--fg-dim);
	}

	.row {
		display: flex;
		gap: 0.4rem;
	}

	.player-empty input {
		flex: 1;
		min-width: 320px;
		background: var(--bg);
		border: 1px solid var(--border);
		color: inherit;
		padding: 0.4rem 0.6rem;
		border-radius: 3px;
		font-family: inherit;
		font-size: 0.9rem;
	}

	.player-empty input:focus {
		outline: 2px solid #5b8def;
		border-color: transparent;
	}

	.player-empty button {
		background: var(--bg-elev);
		color: inherit;
		border: 1px solid var(--border-strong);
		padding: 0.4rem 1rem;
		border-radius: 3px;
		cursor: pointer;
		font-family: inherit;
	}

	.bbs {
		/* BBS ペイン全体 (フィルタ行 + レス一覧 + 未選択メッセージ) を
		   白系の島にする。スレッド帯 / ステータスバー / 書き込み欄
		   まわりだけが黒。 */
		background: var(--bg-elev);
		border-left: 1px solid var(--border);
		overflow-y: auto;
		min-height: 0;
		font-size: 0.85rem;
		color: var(--fg);
	}

	.bbs-empty {
		padding: 1rem;
	}

	.thread-list {
		list-style: none;
		padding: 0;
		margin: 0.5rem 0 0;
	}

	.thread-item {
		display: block;
		width: 100%;
		text-align: left;
		background: transparent;
		color: inherit;
		border: none;
		padding: 0.4rem 0.5rem;
		cursor: pointer;
		border-radius: 3px;
	}

	.thread-item:hover {
		background: var(--border);
	}

	.t-title {
		color: var(--fg-dim);
	}

	.t-count {
		color: var(--fg-muted);
		font-size: 0.8rem;
		margin-left: 0.4rem;
	}

	.posts {
		list-style: none;
		padding: 0;
		margin: 0;
		/* 白系の島 (周囲は黒)。テーマに追従。 */
		background: var(--bg-elev);
		color: var(--fg);
	}

	.post {
		padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--border);
		transition: background-color 80ms;
	}
	.post.highlight-id {
		background: color-mix(in srgb, var(--accent) 18%, transparent);
	}

	.head {
		font-size: 0.75rem;
		color: var(--fg-muted);
		margin-bottom: 0.2rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.num {
		color: var(--accent-num);
		font-weight: 600;
		cursor: pointer;
		text-decoration: none;
	}
	.num:hover {
		text-decoration: underline;
	}

	.name {
		color: var(--accent-name);
	}

	.body {
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 0.85rem;
		line-height: 1.4;
	}

	.thread-bar {
		background: var(--bar-thread);
		color: #fff;
		display: flex;
		align-items: stretch;
		font-size: 0.85rem;
		overflow: hidden;
	}

	.thread-bar-button {
		flex: 1;
		display: flex;
		align-items: center;
		gap: 0.4rem;
		background: transparent;
		color: inherit;
		border: none;
		padding: 0 0.7rem;
		text-align: left;
		font-family: inherit;
		font-size: inherit;
		cursor: pointer;
		overflow: hidden;
		white-space: nowrap;
	}

	.thread-bar-button:disabled {
		cursor: default;
	}

	.thread-bar-button:hover:not(:disabled) {
		background: rgba(0, 0, 0, 0.15);
	}

	.t-title-main {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	.t-count-main {
		color: rgba(255, 255, 255, 0.85);
		font-size: 0.8rem;
	}

	.t-grow {
		flex: 1;
	}

	.t-list {
		color: rgba(255, 255, 255, 0.85);
	}
	.t-refresh {
		color: rgba(255, 255, 255, 0.75);
		font-size: 0.72rem;
		min-width: 2.5rem;
		text-align: right;
	}

	.write-box {
		background: var(--bar-write);
		display: grid;
		grid-template-columns: 1fr 32px;
		align-items: stretch;
		padding: 0;
	}

	.write-box textarea {
		background: var(--bg-input);
		color: var(--fg);
		border: none;
		padding: 0.3rem 0.6rem;
		font-family: inherit;
		font-size: 0.85rem;
		resize: none;
		outline: none;
		line-height: 1.4;
	}

	.write-box .send {
		background: transparent;
		color: var(--fg-dim);
		border: none;
		cursor: pointer;
		font-size: 1.1rem;
	}

	.write-box .send:disabled {
		color: #4a4d54;
		cursor: default;
	}

	.status-bar {
		background: var(--bar-status);
		color: #fff;
		display: flex;
		align-items: center;
		padding: 0 0.7rem;
		font-size: 0.78rem;
		gap: 0.6rem;
		overflow: hidden;
		white-space: nowrap;
	}

	.s-name {
		font-weight: 600;
	}

	.s-info,
	.s-up,
	.s-size,
	.s-vol {
		color: rgba(255, 255, 255, 0.85);
	}
	.s-vol {
		min-width: 3rem;
		text-align: right;
	}

	.s-actions {
		margin-left: auto;
		display: flex;
		gap: 0.3rem;
	}

	.s-actions button {
		background: rgba(0, 0, 0, 0.3);
		color: inherit;
		border: 1px solid rgba(255, 255, 255, 0.2);
		border-radius: 3px;
		padding: 0 0.4rem;
		font-size: 0.85rem;
		cursor: pointer;
		line-height: 1.4;
	}

	.s-actions button:hover {
		background: rgba(0, 0, 0, 0.5);
	}

	.muted {
		color: var(--fg-muted);
	}

	/* 黒地の帯の中では .muted も白系で見せる (var(--fg-muted) は
	   黒背景だと潰れるため)。 */
	.thread-bar .muted,
	.status-bar .muted {
		color: rgba(255, 255, 255, 0.75);
	}

	.small {
		font-size: 0.75rem;
		padding: 0.3rem 0.6rem;
	}

	.hint {
		color: var(--fg-dim);
		margin: 0 0 0.4rem;
	}

	.err {
		color: var(--err);
		margin-left: auto;
	}

	/* Linked anchors / IDs inside post bodies and header. */
	:global(.posts a.anchor),
	:global(.posts a.id-link) {
		color: #88c4ff;
		text-decoration: none;
		cursor: pointer;
	}
	:global(.posts a.anchor:hover),
	:global(.posts a.id-link:hover) {
		text-decoration: underline;
	}
	:global(.posts a.external) {
		color: #cfa84d;
		text-decoration: none;
		word-break: break-all;
	}
	:global(.posts a.external:hover) {
		text-decoration: underline;
	}

	/* Anchor/ID popup */
	.popup-backdrop {
		position: fixed;
		inset: 0;
		background: rgba(0, 0, 0, 0.25);
		z-index: 100;
	}
	.popup {
		position: absolute;
		width: 360px;
		max-height: 300px;
		overflow-y: auto;
		background: var(--bg-elev);
		border: 1px solid var(--border-strong);
		border-radius: 6px;
		box-shadow: 0 6px 20px rgba(0, 0, 0, 0.6);
	}
	.popup-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.6rem;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
		font-size: 0.78rem;
		color: var(--fg-dim);
		position: sticky;
		top: 0;
	}
	.popup-head span {
		flex: 1;
	}
	.popup-close {
		background: transparent;
		color: var(--fg-dim);
		border: none;
		font-size: 1.1rem;
		cursor: pointer;
		line-height: 1;
		padding: 0 0.3rem;
	}
	.popup :global(.posts.in-popup) {
		font-size: 0.8rem;
	}

	/* Right-click context menu */
	.ctx-backdrop {
		position: fixed;
		inset: 0;
		z-index: 200;
	}
	.ctx {
		position: absolute;
		min-width: 200px;
		background: var(--bg-elev);
		border: 1px solid var(--border-strong);
		border-radius: 6px;
		padding: 0.25rem 0;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
	}
	.ctx-item {
		display: block;
		width: 100%;
		text-align: left;
		background: transparent;
		color: var(--fg);
		border: none;
		padding: 0.45rem 0.85rem;
		font-family: inherit;
		font-size: 0.88rem;
		cursor: pointer;
	}
	.ctx-item:hover:not(:disabled) {
		background: var(--border);
	}
	.ctx-item:disabled {
		opacity: 0.45;
		cursor: default;
	}
	.ctx-sep {
		height: 1px;
		background: var(--border);
		margin: 0.25rem 0;
	}

	.filter-bar {
		display: flex;
		gap: 0.3rem;
		padding: 0.3rem 0.5rem;
		border-bottom: 1px solid var(--border);
		/* レス一覧の島の上に sticky で乗るので、posts と同じ背景に合わせる */
		background: var(--bg-elev);
		color: var(--fg);
		position: sticky;
		top: 0;
		z-index: 5;
		align-items: center;
	}
	.filter-bar input[type='search'] {
		flex: 1;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.25rem 0.45rem;
		font-family: inherit;
		font-size: 0.8rem;
	}
	.filter-bar input[type='search']:focus {
		outline: 2px solid var(--accent);
		border-color: transparent;
	}
	.filter-stat {
		font-size: 0.72rem;
		color: var(--fg-muted);
	}
	.filter-clear {
		background: transparent;
		color: var(--fg-dim);
		border: none;
		cursor: pointer;
		font-size: 0.9rem;
		line-height: 1;
		padding: 0 0.3rem;
	}
</style>
