<script lang="ts">
	import { onDestroy } from 'svelte';
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
	import { formatUptime, renderBodyHtml } from '$lib/format';

	// ── State ────────────────────────────────────────────────────────

	let pasteUrl = $state('');
	let busy = $state(false);
	let lastError = $state<string | null>(null);

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

	// Polling handles
	let infoTimer: ReturnType<typeof setInterval> | null = null;
	let threadTimer: ReturnType<typeof setInterval> | null = null;

	onDestroy(() => {
		if (infoTimer) clearInterval(infoTimer);
		if (threadTimer) clearInterval(threadTimer);
	});

	// ── Derived ──────────────────────────────────────────────────────

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
			} else {
				posts = [...posts, ...newPosts];
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

	function onWriteKey(e: KeyboardEvent) {
		// Ctrl/Cmd + Enter to send (UI design §書き込みテキストボックス).
		if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			onSubmit();
		}
	}

	function errorMessage(e: unknown): string {
		if (e instanceof CommandError) return `${e.code}: ${e.message}`;
		return String(e);
	}
</script>

<svelte:head>
	<title>PSTPlayer</title>
</svelte:head>

<div class="app">
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
				<ol class="posts">
					{#each posts as p (p.number)}
						<li class="post">
							<div class="head">
								<span class="num">{p.number}</span>
								<span class="name">{p.name}</span>
								{#if p.mail}<span class="mail">[{p.mail}]</span>{/if}
								<span class="date">{p.date}</span>
								{#if p.id}<span class="id">ID:{p.id}</span>{/if}
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
		{#if currentThreadUrl}
			<span class="t-title-main">
				{posts[0]?.threadTitle || currentThreadUrl}
			</span>
			<span class="t-count-main">({posts.length})</span>
		{:else}
			<span class="muted">— スレッド未選択 —</span>
		{/if}
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

	<!-- Status bar (緑) -->
	<div class="status-bar">
		{#if statusLine}
			<span class="s-name">{statusLine.name}</span>
			<span class="s-info">{statusLine.br} {statusLine.ldir} {statusLine.lrel}</span>
			<span class="s-up">{statusLine.up}</span>
			<span class="s-actions">
				<button onclick={onBump} title="再接続 (Bump)">↻</button>
				<button onclick={onStop} title="切断 (Stop)">■</button>
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
		background: #1d1f23;
		color: #e8eaed;
	}

	.app {
		display: grid;
		grid-template-rows: 1fr 24px auto 24px;
		height: 100vh;
		overflow: hidden;
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
		background: #14161a;
		padding: 0.3rem 0.5rem;
		border-radius: 3px;
		max-width: 80%;
		overflow-wrap: anywhere;
	}

	.url-form label {
		display: block;
		margin-bottom: 0.5rem;
		font-size: 0.9rem;
		color: #c6c9d0;
	}

	.row {
		display: flex;
		gap: 0.4rem;
	}

	.player-empty input {
		flex: 1;
		min-width: 320px;
		background: #1d1f23;
		border: 1px solid #3a3d44;
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
		background: #3a3d44;
		color: inherit;
		border: 1px solid #4a4d54;
		padding: 0.4rem 1rem;
		border-radius: 3px;
		cursor: pointer;
		font-family: inherit;
	}

	.bbs {
		background: #1d1f23;
		border-left: 1px solid #2c2f34;
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
		background: #2c2f34;
	}

	.t-title {
		color: #cfd2d9;
	}

	.t-count {
		color: #8a8d94;
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
		border-bottom: 1px solid #25282d;
	}

	.head {
		font-size: 0.75rem;
		color: #8a8d94;
		margin-bottom: 0.2rem;
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.num {
		color: #5b8def;
		font-weight: 600;
	}

	.name {
		color: #97e09e;
	}

	.body {
		white-space: pre-wrap;
		word-break: break-word;
		font-size: 0.85rem;
		line-height: 1.4;
	}

	.thread-bar {
		background: #c87a2e;
		color: #fff;
		display: flex;
		align-items: center;
		padding: 0 0.7rem;
		font-size: 0.85rem;
		gap: 0.4rem;
		overflow: hidden;
		white-space: nowrap;
		text-overflow: ellipsis;
	}

	.t-title-main {
		font-weight: 600;
	}

	.t-count-main {
		color: rgba(255, 255, 255, 0.8);
		font-size: 0.8rem;
	}

	.write-box {
		background: #0c0d10;
		display: grid;
		grid-template-columns: 1fr 32px;
		align-items: stretch;
		padding: 0;
	}

	.write-box textarea {
		background: transparent;
		color: #e8eaed;
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
		color: #c6c9d0;
		border: none;
		cursor: pointer;
		font-size: 1.1rem;
	}

	.write-box .send:disabled {
		color: #4a4d54;
		cursor: default;
	}

	.status-bar {
		background: #2d6b3b;
		color: #e8eaed;
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
		color: #8a8d94;
	}

	.small {
		font-size: 0.75rem;
		padding: 0.3rem 0.6rem;
	}

	.hint {
		color: #c6c9d0;
		margin: 0 0 0.4rem;
	}

	.err {
		color: #f08c8c;
		margin-left: auto;
	}
</style>
