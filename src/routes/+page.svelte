<script lang="ts">
	import { onDestroy, onMount } from 'svelte';
	import { listen, type UnlistenFn } from '@tauri-apps/api/event';
	import {
		CommandError,
		bumpChannel,
		endpointForUrl,
		fetchChannelInfo,
		fetchChannelStatus,
		fetchThread,
		listThreads,
		postToThread,
		resolveStreamUrl,
		stopChannel,
		type ChannelInfo,
		type ChannelStatus,
		type FetchState,
		type PeerCastEndpoint,
		type Post,
		type SubjectEntry,
	} from '$lib/api';
	import { formatUptime, renderBodyHtml, renderIdHtml } from '$lib/format';
	import { openSettings, openThreadList } from '$lib/windows';
	import { installShortcuts, setAlwaysOnTop, setDecorations } from '$lib/shortcuts';
	import { notify } from '$lib/notifications';
	import { initTheme } from '$lib/theme';

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

	// Polling handles
	let infoTimer: ReturnType<typeof setInterval> | null = null;
	let threadTimer: ReturnType<typeof setInterval> | null = null;
	let threadSelectedUnlisten: UnlistenFn | null = null;

	let shortcutsUnlisten: (() => void) | null = null;
	let themeUnlisten: (() => void) | null = null;

	onMount(async () => {
		themeUnlisten = initTheme();

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
		});
	});

	onDestroy(() => {
		if (infoTimer) clearInterval(infoTimer);
		if (threadTimer) clearInterval(threadTimer);
		threadSelectedUnlisten?.();
		shortcutsUnlisten?.();
		themeUnlisten?.();
	});

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
		const br =
			channelStatus && channelInfo.bitrate
				? `${channelInfo.bitrate} kbps`
				: channelInfo.bitrate
					? `${channelInfo.bitrate} kbps`
					: '-';
		const up = channelStatus ? formatUptime(channelStatus.uptime) : '-';
		const ldir = channelStatus ? `L:${channelStatus.localDirects}` : '';
		const lrel = channelStatus ? `R:${channelStatus.localRelays}` : '';
		return { name, br, up, ldir, lrel };
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

			if (channelId && endpoint) {
				await reloadInfoAndBbs();
				startPolling();
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

	async function tryLoadBoard(contactUrl: string) {
		try {
			threadList = await listThreads(contactUrl);
			// Auto-pick the contact URL if it already names a thread (read.cgi…).
			const m = contactUrl.match(/\/(\d+)\/?$/);
			if (m) {
				currentThreadUrl = contactUrl;
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
			if (forceReset || !fetchState) {
				posts = newPosts;
			} else if (newPosts.length > 0) {
				posts = [...posts, ...newPosts];
				const preview = newPosts[0].body.replace(/\s+/g, ' ').slice(0, 80);
				const title = `新着 ${newPosts.length} 件 / ${posts[0]?.threadTitle || ''}`;
				notify(title, preview);
			}
			fetchState = newState;
		} catch (e) {
			lastError = errorMessage(e);
		} finally {
			threadLoading = false;
		}
	}

	function startPolling() {
		if (infoTimer) clearInterval(infoTimer);
		if (threadTimer) clearInterval(threadTimer);
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
		}, 5_000);
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
		// Ctrl/Cmd + Enter to send (UI design §書き込みテキストボックス).
		if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			onSubmit();
		}
	}

	function onPostsClick(e: MouseEvent) {
		const t = e.target;
		if (!(t instanceof HTMLElement)) return;
		const from = t.dataset.anchorFrom;
		const to = t.dataset.anchorTo;
		const id = t.dataset.idLink;
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
		<div class="player">
			{#if streamUrl}
				<div class="player-placeholder">
					<div>
						<div class="big">▶ libmpv プレースホルダ</div>
						<div class="hint">ストリーム URL を取得済み (libmpv 統合は次フェーズ):</div>
						<code class="url">{streamUrl}</code>
					</div>
				</div>
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
				<ol class="posts" onclick={onPostsClick}>
					{#each visiblePosts as p (p.number)}
						<li class="post">
							<div class="head">
								<span class="num">{p.number}</span>
								<span class="name">{p.name}</span>
								{#if p.mail}<span class="mail">[{p.mail}]</span>{/if}
								<span class="date">{p.date}</span>
								{#if p.id}<span class="id">{@html renderIdHtml(p.id)}</span>{/if}
							</div>
							<div class="body">{@html renderBodyHtml(p.body)}</div>
						</li>
					{/each}
				</ol>
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
			<span class="t-list">≡</span>
		</button>
	</div>

	<!-- Write box (黒) -->
	<div class="write-box">
		<textarea
			placeholder={currentThreadUrl ? 'ここに書き込む  (Ctrl/Cmd+Enter で送信)' : '書き込み欄'}
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

	<!-- Anchor / ID popup overlay -->
	{#if popup}
		<div
			class="popup-backdrop"
			role="dialog"
			tabindex="-1"
			onclick={closePopup}
			onkeydown={onPopupKey}
		>
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
				<ol class="posts in-popup" onclick={onPostsClick}>
					{#each popup.posts as p (p.number)}
						<li class="post">
							<div class="head">
								<span class="num">{p.number}</span>
								<span class="name">{p.name}</span>
								<span class="date">{p.date}</span>
								{#if p.id}<span class="id">{@html renderIdHtml(p.id)}</span>{/if}
							</div>
							<div class="body">{@html renderBodyHtml(p.body)}</div>
						</li>
					{/each}
				</ol>
			</div>
		</div>
	{/if}

	<!-- Status bar (緑) -->
	<div class="status-bar">
		{#if statusLine}
			<span class="s-name">{statusLine.name}</span>
			<span class="s-info">{statusLine.br} {statusLine.ldir} {statusLine.lrel}</span>
			<span class="s-up">{statusLine.up}</span>
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

	.player-placeholder,
	.player-empty {
		color: #9aa0a6;
		text-align: center;
		padding: 2rem;
	}

	.big {
		font-size: 1.4rem;
		margin-bottom: 0.5rem;
	}

	.url {
		display: inline-block;
		margin-top: 0.5rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		background: var(--bg-input);
		padding: 0.3rem 0.5rem;
		border-radius: 3px;
		max-width: 80%;
		overflow-wrap: anywhere;
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
		background: var(--bg);
		border-left: 1px solid var(--border);
		overflow-y: auto;
		min-height: 0;
		font-size: 0.85rem;
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
	}

	.post {
		padding: 0.5rem 0.6rem;
		border-bottom: 1px solid var(--border);
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

	.write-box {
		background: var(--bar-write);
		display: grid;
		grid-template-columns: 1fr 32px;
		align-items: stretch;
		padding: 0;
	}

	.write-box textarea {
		background: transparent;
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
		color: var(--fg);
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
	.s-up {
		color: rgba(255, 255, 255, 0.85);
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

	.filter-bar {
		display: flex;
		gap: 0.3rem;
		padding: 0.3rem 0.5rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg);
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
