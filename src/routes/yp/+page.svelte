<script lang="ts">
	import { onMount } from 'svelte';
	import {
		CommandError,
		fetchYpIndex,
		getConfig,
		matchYpEntry,
		spawnViewer,
		type FavoriteRule,
		type YpEntry,
	} from '$lib/api';

	let entries = $state<YpEntry[]>([]);
	let loading = $state(false);
	let error = $state<string | null>(null);
	let ypUrl = $state<string>('');
	let filter = $state('');
	let sortKey = $state<'listeners' | 'name' | 'genre' | 'bitrate'>('listeners');
	let sortDesc = $state(true);
	let favorites = $state<FavoriteRule[]>([]);

	onMount(async () => {
		try {
			const cfg = await getConfig();
			ypUrl = cfg?.peercast?.ypUrl ?? '';
			favorites = cfg?.favorites?.rules ?? [];
		} catch {
			/* default to empty; refresh will surface the error */
		}
		await refresh();
	});

	function matchFor(e: YpEntry): FavoriteRule | null {
		return matchYpEntry(favorites, e);
	}

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

	// 行クリック: 視聴用 pstplayer を別プロセスで spawn する。同じ
	// channel_id が既に視聴中なら既存ウィンドウにフォーカスのみ移す
	// (ADR-0006 Step 4)。YP ウィンドウ自身は閉じない (ハブとして残す)。
	async function pick(entry: YpEntry) {
		try {
			await spawnViewer(entry.id);
		} catch (e) {
			console.warn('spawn_viewer failed', e);
		}
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
		// action='block' は完全に隠す、'ignore' も一覧から除外。
		const base = entries.filter((e) => {
			const a = matchFor(e)?.action ?? 'show';
			return a === 'show';
		});
		const filtered = q
			? base.filter(
					(e) =>
						e.name.toLowerCase().includes(q) ||
						e.genre.toLowerCase().includes(q) ||
						e.desc.toLowerCase().includes(q) ||
						e.comment.toLowerCase().includes(q),
				)
			: base.slice();
		filtered.sort((a, b) => {
			// pin_top のお気に入りは常に最上位に固める (列ソートより優先)。
			const pa = matchFor(a)?.pin_top ? 1 : 0;
			const pb = matchFor(b)?.pin_top ? 1 : 0;
			if (pa !== pb) return pb - pa;
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
				{@const fav = matchFor(e)}
				{@const bg = fav?.background || fav?.color || ''}
				<button
					class="row"
					class:pinned={fav?.pin_top}
					onclick={() => pick(e)}
					title={fav
						? `★ ${fav.name || 'お気に入り'}${fav.auto_record ? ' / 自動録画' : ''}`
						: e.desc || e.comment}
					style={[
						bg ? `background:${bg};` : '',
						fav?.text_color ? `color:${fav.text_color};` : '',
					].join('')}
				>
					<span class="c-listeners">{e.listeners}</span>
					<span class="c-name">
						{#if fav}<span class="fav-mark">★</span>{/if}
						{e.name || '(unnamed)'}
					</span>
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

	.row.pinned {
		font-weight: 600;
	}
	.fav-mark {
		color: var(--accent, #ff8a3d);
		margin-right: 0.25rem;
	}

	.muted {
		color: var(--fg-muted);
	}
	.small {
		font-size: 0.8rem;
		padding: 0.7rem;
	}
</style>
