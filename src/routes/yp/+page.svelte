<script lang="ts">
	import { onMount } from 'svelte';
	import { emit } from '@tauri-apps/api/event';
	import { CommandError, fetchYpIndex, getConfig, type YpEntry } from '$lib/api';

	let entries = $state<YpEntry[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let ypUrl = $state<string>('');
	let filter = $state('');
	let sortKey = $state<'listeners' | 'name' | 'genre' | 'bitrate'>('listeners');
	let sortDesc = $state(true);

	onMount(async () => {
		try {
			const cfg = await getConfig();
			ypUrl = cfg?.peercast?.ypUrl ?? '';
		} catch {
			/* default to empty; refresh will surface the error */
		}
		await refresh();
	});

	async function refresh() {
		loading = true;
		error = null;
		try {
			entries = await fetchYpIndex();
		} catch (e) {
			error = e instanceof CommandError ? `${e.code}: ${e.message}` : String(e);
		} finally {
			loading = false;
		}
	}

	// 行クリック: 「このチャンネルを開け」を main にイベントで通知。
	// PeerCast URL は `http://{host:port}/pls/{id}` の形に組み立てる。
	// host:port は YP の `tip` フィールド (チャンネル元の IP) ではなく
	// ユーザーの PeerCast (= 自分が接続しているリレー) で再生するので、
	// main 側で endpoint を決め直す。ここでは pls URL の起点として
	// `http://{tip}/pls/{id}` を渡すが、main は `endpointForUrl` で
	// 自分の config endpoint に丸めて使う。
	async function pick(entry: YpEntry) {
		const url = `http://${entry.tip}/pls/${entry.id}`;
		await emit('yp:selected', { url, channelName: entry.name });
	}

	function toggleSort(key: typeof sortKey) {
		if (sortKey === key) {
			sortDesc = !sortDesc;
		} else {
			sortKey = key;
			sortDesc = true;
		}
	}

	const visible = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		const filtered = q
			? entries.filter(
					(e) =>
						e.name.toLowerCase().includes(q) ||
						e.genre.toLowerCase().includes(q) ||
						e.desc.toLowerCase().includes(q) ||
						e.comment.toLowerCase().includes(q),
				)
			: entries.slice();
		filtered.sort((a, b) => {
			let cmp: number;
			switch (sortKey) {
				case 'listeners':
					cmp = a.listeners - b.listeners;
					break;
				case 'bitrate':
					cmp = a.bitrate - b.bitrate;
					break;
				case 'name':
					cmp = a.name.localeCompare(b.name, 'ja');
					break;
				case 'genre':
					cmp = a.genre.localeCompare(b.genre, 'ja');
					break;
			}
			return sortDesc ? -cmp : cmp;
		});
		return filtered;
	});

	function arrow(key: typeof sortKey): string {
		if (sortKey !== key) return '';
		return sortDesc ? ' ▼' : ' ▲';
	}
</script>

<svelte:head>
	<title>PSTPlayer · YP チャンネル一覧</title>
</svelte:head>

<main>
	<header>
		<input
			class="search"
			type="search"
			bind:value={filter}
			placeholder="絞り込み (名前 / ジャンル / 詳細)"
		/>
		<span class="stat">{visible.length} / {entries.length}</span>
		<button onclick={refresh} disabled={loading}>{loading ? '更新中…' : '↻ 更新'}</button>
	</header>
	{#if ypUrl}
		<div class="src" title={ypUrl}>取得元: {ypUrl}</div>
	{:else}
		<div class="src warn">設定 → PeerCast → 「YP index.txt URL」を設定してください。</div>
	{/if}

	{#if error}
		<div class="err">⚠ {error}</div>
	{/if}

	<div class="table">
		<div class="thead">
			<button class="th th-listeners" onclick={() => toggleSort('listeners')}>
				👥{arrow('listeners')}
			</button>
			<button class="th th-name" onclick={() => toggleSort('name')}>
				名前{arrow('name')}
			</button>
			<button class="th th-genre" onclick={() => toggleSort('genre')}>
				ジャンル{arrow('genre')}
			</button>
			<button class="th th-bitrate" onclick={() => toggleSort('bitrate')}>
				kbps{arrow('bitrate')}
			</button>
			<span class="th th-uptime">経過</span>
		</div>
		<div class="tbody">
			{#each visible as e (e.id)}
				<button class="row" onclick={() => pick(e)} title={e.desc || e.comment}>
					<span class="c-listeners">{e.listeners}</span>
					<span class="c-name">{e.name || '(unnamed)'}</span>
					<span class="c-genre">{e.genre}</span>
					<span class="c-bitrate">{e.bitrate}</span>
					<span class="c-uptime">{e.uptime}</span>
				</button>
			{/each}
			{#if !loading && entries.length === 0 && !error}
				<div class="muted small">
					{ypUrl ? 'チャンネルがありません。' : 'YP URL が未設定です。'}
				</div>
			{/if}
		</div>
	</div>
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
		grid-template-rows: auto auto auto 1fr;
		height: 100vh;
	}

	header {
		display: flex;
		align-items: center;
		gap: 0.5rem;
		padding: 0.4rem 0.6rem;
		border-bottom: 1px solid var(--border);
	}

	.search {
		flex: 1;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.25rem 0.45rem;
		font-family: inherit;
		font-size: 0.85rem;
	}
	.stat {
		font-size: 0.78rem;
		color: var(--fg-muted);
		min-width: 5.5rem;
		text-align: right;
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

	.src {
		font-size: 0.72rem;
		color: var(--fg-muted);
		padding: 0.2rem 0.7rem;
		border-bottom: 1px solid var(--border);
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	.src.warn {
		color: var(--err);
	}

	.err {
		color: var(--err);
		padding: 0.5rem 0.7rem;
		font-size: 0.85rem;
	}

	.table {
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-height: 0;
	}

	.thead,
	.row {
		display: grid;
		grid-template-columns: 3rem 1fr 7rem 4rem 5rem;
		gap: 0.5rem;
		align-items: center;
	}

	.thead {
		padding: 0.3rem 0.7rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elev);
		font-size: 0.78rem;
		color: var(--fg-muted);
		position: sticky;
		top: 0;
	}
	.th {
		background: transparent;
		color: inherit;
		border: none;
		text-align: left;
		font: inherit;
		cursor: pointer;
		padding: 0;
	}
	.th-listeners {
		text-align: right;
	}
	.th-bitrate {
		text-align: right;
	}

	.tbody {
		overflow-y: auto;
		min-height: 0;
	}

	.row {
		width: 100%;
		text-align: left;
		background: transparent;
		color: inherit;
		border: none;
		padding: 0.4rem 0.7rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.85rem;
		border-bottom: 1px solid var(--border);
	}
	.row:hover {
		background: var(--border);
	}

	.c-listeners,
	.c-bitrate {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}
	.c-name {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.c-genre,
	.c-uptime {
		color: var(--fg-muted);
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.muted {
		color: var(--fg-muted);
	}
	.small {
		font-size: 0.8rem;
		padding: 0.7rem;
	}
</style>
