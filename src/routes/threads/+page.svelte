<script lang="ts">
	import { onMount } from 'svelte';
	import { emit } from '@tauri-apps/api/event';
	import { CommandError, listThreads, type SubjectEntry } from '$lib/api';

	let threads = $state<SubjectEntry[]>([]);
	let boardUrl = $state<string>('');
	let loading = $state(false);
	let error = $state<string | null>(null);

	onMount(async () => {
		const params = new URLSearchParams(window.location.search);
		boardUrl = params.get('board') ?? '';
		if (boardUrl) await refresh();
	});

	async function refresh() {
		loading = true;
		error = null;
		try {
			threads = await listThreads(boardUrl);
		} catch (e) {
			error = e instanceof CommandError ? `${e.code}: ${e.message}` : String(e);
		} finally {
			loading = false;
		}
	}

	async function pick(entry: SubjectEntry) {
		// Tell the main window which thread was chosen. Main listens
		// via window.listen('thread:selected').
		await emit('thread:selected', { boardUrl, key: entry.key, title: entry.title });
	}
</script>

<svelte:head>
	<title>PSTPlayer · スレッド一覧</title>
</svelte:head>

<main>
	<header>
		<div class="board" title={boardUrl}>{boardUrl || '(no board)'}</div>
		<button onclick={refresh} disabled={loading}>{loading ? '更新中…' : '↻ 更新'}</button>
	</header>

	{#if error}
		<div class="err">⚠ {error}</div>
	{/if}

	<ul class="list">
		{#each threads as t (t.key)}
			<li>
				<button class="row" onclick={() => pick(t)}>
					<span class="title">{t.title}</span>
					<span class="count">({t.count})</span>
				</button>
			</li>
		{/each}
		{#if !loading && threads.length === 0 && !error}
			<li class="muted small">スレッドがありません。</li>
		{/if}
	</ul>
</main>

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

	main {
		display: grid;
		grid-template-rows: auto auto 1fr;
		height: 100vh;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.5rem 0.7rem;
		border-bottom: 1px solid var(--border);
	}

	.board {
		flex: 1;
		font-size: 0.78rem;
		color: var(--fg-muted);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	header button {
		background: var(--bg-elev);
		color: inherit;
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		padding: 0.3rem 0.7rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.85rem;
	}

	header button:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.list {
		list-style: none;
		padding: 0;
		margin: 0;
		overflow-y: auto;
	}

	.row {
		display: flex;
		width: 100%;
		text-align: left;
		background: transparent;
		color: inherit;
		border: none;
		padding: 0.45rem 0.7rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.88rem;
		border-bottom: 1px solid #25282d;
	}

	.row:hover {
		background: var(--border);
	}

	.title {
		flex: 1;
		color: #cfd2d9;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.count {
		color: var(--fg-muted);
		font-size: 0.78rem;
		margin-left: 0.5rem;
	}

	.err {
		color: var(--err);
		padding: 0.5rem 0.7rem;
		font-size: 0.85rem;
	}

	.muted {
		color: var(--fg-muted);
	}

	.small {
		font-size: 0.8rem;
		padding: 0.5rem 0.7rem;
	}
</style>
