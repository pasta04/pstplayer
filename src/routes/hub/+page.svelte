<script lang="ts">
	// PSTPlayer Desktop ハブ画面 (PeCaRecorder 風)。
	// pstplayer を URL 引数なしで起動した時のメイン画面で、
	// YP テーブル + お気に入り適用 + 行クリックで別プロセス視聴起動を行う。
	//
	// 設計は docs/design/pstplayer-hub-*.svg, pstplayer-hub-interactions.md,
	// pstplayer-hub-settings.md を参照。

	import { onMount } from 'svelte';
	import {
		CommandError,
		effectiveBackground,
		fetchYpSources,
		firstFavoriteMatch,
		getConfig,
		listActiveViewers,
		peercastPing,
		spawnViewer,
		type FavoriteAction,
		type FavoriteRule,
		type YpEntry,
		type YpFetchFailure,
	} from '$lib/api';
	import { openSettings, openThreadList } from '$lib/windows';
	import { notify } from '$lib/notifications';

	type SortKey = 'name' | 'genre' | 'listeners' | 'bitrate' | 'uptime' | 'yp_source';
	type TabKey = 'all' | 'favorites' | 'recording' | 'watching' | 'new';

	let entries = $state<YpEntry[]>([]);
	let failures = $state<YpFetchFailure[]>([]);
	let favorites = $state<FavoriteRule[]>([]);
	let loading = $state(false);
	let lastError = $state<string | null>(null);
	let lastUpdatedAt = $state<Date | null>(null);

	let filter = $state('');
	let sortKey = $state<SortKey>('listeners');
	let sortDesc = $state(true);
	let activeTab = $state<TabKey>('all');
	let selectedId = $state<string | null>(null);

	// 前回 fetch 時に見えていた channel_id の集合 (新着判定用)。
	let prevIds = $state<Set<string>>(new Set());
	let newIds = $state<Set<string>>(new Set());
	let watchingIds = $state<Set<string>>(new Set());
	// 既に通知済みの新着 ID。重複通知防止 (同じセッションで何度も
	// 「新着 X」を出さない)。
	let notifiedIds = new Set<string>();
	let firstRefreshDone = false;

	// 自動再 fetch 間隔 (秒)。0 で無効。将来は config から取る。
	const AUTO_REFRESH_SEC = 60;
	const WATCHING_POLL_SEC = 5;
	let refreshTimer: ReturnType<typeof setInterval> | null = null;
	let watchingTimer: ReturnType<typeof setInterval> | null = null;

	let menuOpen = $state(false);
	let menuX = $state(0);
	let menuY = $state(0);
	let menuTarget = $state<YpEntry | null>(null);

	onMount(() => {
		void refresh();
		void refreshWatching();
		if (AUTO_REFRESH_SEC > 0) {
			refreshTimer = setInterval(() => {
				void refresh();
			}, AUTO_REFRESH_SEC * 1000);
		}
		if (WATCHING_POLL_SEC > 0) {
			watchingTimer = setInterval(() => {
				void refreshWatching();
			}, WATCHING_POLL_SEC * 1000);
		}
		return () => {
			if (refreshTimer) clearInterval(refreshTimer);
			if (watchingTimer) clearInterval(watchingTimer);
		};
	});

	async function refreshWatching() {
		try {
			const ids = await listActiveViewers();
			watchingIds = new Set(ids);
		} catch {
			/* ignore: lock 読み取りエラーは無視して次回再試行 */
		}
	}

	async function refresh() {
		loading = true;
		lastError = null;
		try {
			const cfg = await getConfig();
			favorites = cfg?.favorites?.rules ?? [];
			try {
				await peercastPing();
			} catch (e) {
				if (e instanceof CommandError && e.code === 'peercast_unreachable') {
					lastError =
						'PeerCast 本体に接続できません。設定 → 接続 で host:port を確認してください。';
				}
			}
			const outcome = await fetchYpSources();
			// 新着 ID 判定
			const currentIds = new Set(outcome.entries.map((e) => e.id));
			const fresh = new Set<string>();
			for (const id of currentIds) if (!prevIds.has(id)) fresh.add(id);
			newIds = fresh;
			prevIds = currentIds;

			entries = outcome.entries;
			failures = outcome.failures;
			lastUpdatedAt = new Date();

			// 新着 + お気に入りマッチ → OS 通知。初回 fetch は「全部新着」に
			// 見えるので通知抑止。同じ ID は重複通知しない。
			if (firstRefreshDone) {
				for (const e of outcome.entries) {
					if (!fresh.has(e.id)) continue;
					if (notifiedIds.has(e.id)) continue;
					const rule = matchFor(e);
					if (!rule) continue;
					if (rule.action !== 'show') continue;
					notifiedIds.add(e.id);
					void notify(`★ ${rule.name || 'お気に入り'} 配信開始`, `${e.name}\n${e.desc}`);
				}
			}
			firstRefreshDone = true;
		} catch (e) {
			lastError = e instanceof Error ? e.message : String(e);
		} finally {
			loading = false;
		}
	}

	function matchFor(e: YpEntry): FavoriteRule | null {
		return firstFavoriteMatch(favorites, {
			name: e.name,
			genre: e.genre,
			desc: e.desc,
			comment: e.comment,
		});
	}

	function actionOf(rule: FavoriteRule | null): FavoriteAction {
		return rule?.action ?? 'show';
	}

	const visible = $derived.by(() => {
		const q = filter.trim().toLowerCase();
		const list = entries
			.map((e) => ({ e, rule: matchFor(e) }))
			.filter(({ rule }) => actionOf(rule) !== 'block') // Block は完全に隠す
			.filter(({ e, rule }) => {
				// タブフィルタ
				if (activeTab === 'favorites' && !rule) return false;
				if (activeTab === 'new' && !newIds.has(e.id)) return false;
				if (activeTab === 'recording') return false; // TODO: 録画中の判定
				if (activeTab === 'watching' && !watchingIds.has(e.id)) return false;
				// Ignore はすべて/お気に入り/新着では非表示にする (専用タブ無いので一旦隠すだけ)
				if (actionOf(rule) === 'ignore') return false;
				// テキストフィルタ
				if (!q) return true;
				return (
					e.name.toLowerCase().includes(q) ||
					e.genre.toLowerCase().includes(q) ||
					e.desc.toLowerCase().includes(q) ||
					e.comment.toLowerCase().includes(q) ||
					e.yp_source.toLowerCase().includes(q)
				);
			});
		// ソート (pin_top は常に上に固定)
		list.sort((a, b) => {
			const pa = a.rule?.pin_top ? 1 : 0;
			const pb = b.rule?.pin_top ? 1 : 0;
			if (pa !== pb) return pb - pa;
			let cmp = 0;
			switch (sortKey) {
				case 'name':
					cmp = a.e.name.localeCompare(b.e.name, 'ja');
					break;
				case 'genre':
					cmp = a.e.genre.localeCompare(b.e.genre, 'ja');
					break;
				case 'listeners':
					cmp = a.e.listeners - b.e.listeners;
					break;
				case 'bitrate':
					cmp = a.e.bitrate - b.e.bitrate;
					break;
				case 'uptime':
					cmp = a.e.uptime.localeCompare(b.e.uptime);
					break;
				case 'yp_source':
					cmp = a.e.yp_source.localeCompare(b.e.yp_source);
					break;
			}
			return sortDesc ? -cmp : cmp;
		});
		return list;
	});

	const counts = $derived.by(() => {
		let all = 0,
			fav = 0,
			fresh = 0,
			watching = 0;
		for (const e of entries) {
			const rule = matchFor(e);
			if (actionOf(rule) === 'block') continue;
			if (actionOf(rule) === 'ignore') continue;
			all++;
			if (rule) fav++;
			if (newIds.has(e.id)) fresh++;
			if (watchingIds.has(e.id)) watching++;
		}
		return { all, fav, fresh, watching };
	});

	function toggleSort(k: SortKey) {
		if (sortKey === k) {
			sortDesc = !sortDesc;
		} else {
			sortKey = k;
			sortDesc = true;
		}
	}

	function arrow(k: SortKey) {
		if (sortKey !== k) return '';
		return sortDesc ? ' ▼' : ' ▲';
	}

	async function watchRow(e: YpEntry) {
		closeMenu();
		try {
			await spawnViewer(e.id);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	function onRowClick(e: YpEntry) {
		selectedId = e.id;
	}

	function onRowDblClick(e: YpEntry) {
		watchRow(e);
	}

	function onRowContextMenu(ev: MouseEvent, e: YpEntry) {
		ev.preventDefault();
		selectedId = e.id;
		menuTarget = e;
		menuX = ev.clientX;
		menuY = ev.clientY;
		menuOpen = true;
	}

	function closeMenu() {
		menuOpen = false;
		menuTarget = null;
	}

	async function copy(text: string) {
		try {
			await navigator.clipboard.writeText(text);
		} catch {
			/* ignore */
		}
		closeMenu();
	}

	function openInBrowser(url: string) {
		if (!url) return;
		// tauri-plugin-opener: window.__TAURI__ etc. 経由 / 簡易には a タグ click
		void import('@tauri-apps/plugin-opener').then((m) => m.openUrl(url)).catch(() => undefined);
		closeMenu();
	}

	async function openBbs(url: string) {
		if (!url) return;
		await openThreadList(url);
		closeMenu();
	}

	function plsUrlFor(e: YpEntry, host: string, port: number) {
		return `http://${host}:${port}/pls/${e.id}`;
	}

	// 現在 PeerCast host:port (コピー用)。refresh 時に lazy に取り直す。
	let currentPeerHost = $state('localhost');
	let currentPeerPort = $state(7144);
	onMount(async () => {
		try {
			const cfg = await getConfig();
			currentPeerHost = cfg?.peercast?.host ?? 'localhost';
			currentPeerPort = cfg?.peercast?.port ?? 7144;
		} catch {
			/* defaults */
		}
	});

	function onKeydown(ev: KeyboardEvent) {
		// テキスト入力中はショートカット無効
		const target = ev.target as HTMLElement | null;
		const tag = target?.tagName?.toLowerCase();
		if (tag === 'input' || tag === 'textarea' || tag === 'select') {
			// ただし Esc はメニュー閉じる用途で受ける
			if (ev.key === 'Escape') {
				if (menuOpen) closeMenu();
			}
			return;
		}
		if (ev.key === 'F5' || ((ev.ctrlKey || ev.metaKey) && ev.key === 'r')) {
			ev.preventDefault();
			void refresh();
		} else if ((ev.ctrlKey || ev.metaKey) && ev.key === 'f') {
			ev.preventDefault();
			const inp = document.querySelector<HTMLInputElement>('input.filter');
			inp?.focus();
		} else if (ev.key === 'Escape') {
			if (menuOpen) closeMenu();
		} else if (ev.key === 'Enter') {
			const e = visible.find((v) => v.e.id === selectedId)?.e;
			if (e) void watchRow(e);
		} else if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
			if (visible.length === 0) return;
			ev.preventDefault();
			const idx = visible.findIndex((v) => v.e.id === selectedId);
			const dir = ev.key === 'ArrowDown' ? 1 : -1;
			const next = idx < 0 ? 0 : Math.min(visible.length - 1, Math.max(0, idx + dir));
			selectedId = visible[next].e.id;
			// 選択行を画面内に
			document
				.querySelector<HTMLElement>('tr.selected')
				?.scrollIntoView({ block: 'nearest', behavior: 'instant' });
		}
	}

	function fmtTime(d: Date | null) {
		if (!d) return '';
		const hh = String(d.getHours()).padStart(2, '0');
		const mm = String(d.getMinutes()).padStart(2, '0');
		const ss = String(d.getSeconds()).padStart(2, '0');
		return `${hh}:${mm}:${ss}`;
	}
</script>

<svelte:head>
	<title>PSTPlayer · ハブ</title>
</svelte:head>

<svelte:window
	onclick={() => {
		if (menuOpen) closeMenu();
	}}
	onkeydown={onKeydown}
/>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<main onclick={() => closeMenu()}>
	<header class="toolbar">
		<button onclick={refresh} disabled={loading}>{loading ? '更新中…' : '↻ 更新'}</button>
		<button onclick={openSettings}>⚙ 設定</button>
		<input
			class="filter"
			type="search"
			bind:value={filter}
			placeholder="絞り込み: 名前 / ジャンル / 詳細 / コメント / YP 名"
		/>
		<span class="stat">
			{visible.length} / {counts.all} ch
		</span>
		<span class="updated">最終更新: {fmtTime(lastUpdatedAt)}</span>
	</header>

	<nav class="tabs">
		<button class:active={activeTab === 'all'} onclick={() => (activeTab = 'all')}>
			すべて ({counts.all})
		</button>
		<button class:active={activeTab === 'favorites'} onclick={() => (activeTab = 'favorites')}>
			お気に入り ({counts.fav})
		</button>
		<button class:active={activeTab === 'new'} onclick={() => (activeTab = 'new')}>
			新着 ({counts.fresh})
		</button>
		<button class:active={activeTab === 'recording'} onclick={() => (activeTab = 'recording')}>
			● 録画中 (—)
		</button>
		<button class:active={activeTab === 'watching'} onclick={() => (activeTab = 'watching')}>
			視聴中 ({counts.watching})
		</button>
	</nav>

	{#if lastError}
		<div class="error">⚠ {lastError}</div>
	{/if}

	{#if failures.length > 0}
		<div class="warn">
			YP 取得失敗:
			{#each failures as f}
				<span class="failure">[{f.source}] {f.error}</span>
			{/each}
		</div>
	{/if}

	<div class="table-wrap">
		<table>
			<thead>
				<tr>
					<th class="col-name" onclick={() => toggleSort('name')}>チャンネル名{arrow('name')}</th>
					<th class="col-desc" onclick={() => toggleSort('genre')}
						>ジャンル - 詳細 「コメント」{arrow('genre')}</th
					>
					<th class="col-num" onclick={() => toggleSort('listeners')}>👤{arrow('listeners')}</th>
					<th class="col-num" onclick={() => toggleSort('bitrate')}>kbps{arrow('bitrate')}</th>
					<th class="col-uptime" onclick={() => toggleSort('uptime')}>配信{arrow('uptime')}</th>
					<th class="col-type">形式</th>
					<th class="col-filter">フィルタ</th>
					<th class="col-yp" onclick={() => toggleSort('yp_source')}>YP{arrow('yp_source')}</th>
					<th class="col-contact">コンタクト</th>
				</tr>
			</thead>
			<tbody>
				{#each visible as { e, rule } (e.id + '@' + e.yp_source)}
					{@const bg = effectiveBackground(rule)}
					{@const fg = rule?.text_color ?? ''}
					<tr
						class:selected={selectedId === e.id}
						class:pinned={rule?.pin_top}
						class:newish={newIds.has(e.id)}
						style:background={bg || undefined}
						style:color={fg || undefined}
						onclick={() => onRowClick(e)}
						ondblclick={() => onRowDblClick(e)}
						oncontextmenu={(ev) => onRowContextMenu(ev, e)}
					>
						<td class="col-name">
							{#if rule}<span class="star">★</span>{/if}{e.name}{#if watchingIds.has(e.id)}
								<span class="watching-badge" title="このチャンネルは視聴ウィンドウで開いています"
									>▶</span
								>{/if}
						</td>
						<td class="col-desc">
							{#if e.genre}[{e.genre}]{/if}
							{e.desc}
							{#if e.comment}「{e.comment}」{/if}
						</td>
						<td class="col-num">{e.listeners} / {e.relays}</td>
						<td class="col-num">{e.bitrate}</td>
						<td class="col-uptime">{e.uptime}</td>
						<td class="col-type">{e.content_type}</td>
						<td class="col-filter">{rule?.name ?? ''}</td>
						<td class="col-yp">{e.yp_source}</td>
						<td class="col-contact" title={e.contact_url}>{e.contact_url}</td>
					</tr>
				{/each}
				{#if visible.length === 0}
					<tr>
						<td colspan="9" class="empty">
							{loading ? '読み込み中…' : 'チャンネルがありません'}
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	<footer class="statusbar">
		<span>★ {counts.fav} / 全 {counts.all} ch</span>
		<span class="sep">·</span>
		<span>PeerCast: {currentPeerHost}:{currentPeerPort}</span>
	</footer>
</main>

{#if menuOpen && menuTarget}
	{@const t = menuTarget}
	{@const pls = plsUrlFor(t, currentPeerHost, currentPeerPort)}
	<!-- svelte-ignore a11y_click_events_have_key_events -->
	<div
		class="menu"
		style:left={menuX + 'px'}
		style:top={menuY + 'px'}
		onclick={(ev) => ev.stopPropagation()}
		role="menu"
		tabindex="-1"
	>
		<button onclick={() => watchRow(t)} class="primary">▶ 視聴 (別ウィンドウで開く)</button>
		<hr />
		<button onclick={() => openBbs(t.contact_url)} disabled={!t.contact_url}
			>📺 BBS としてコンタクト URL を開く</button
		>
		<button onclick={() => openInBrowser(t.contact_url)} disabled={!t.contact_url}
			>🌐 コンタクト URL をブラウザで開く</button
		>
		<hr />
		<div class="submenu-label">📋 コピー</div>
		<button class="indent" onclick={() => copy(t.name)}>チャンネル名</button>
		<button class="indent" onclick={() => copy(`[${t.genre}] ${t.desc} 「${t.comment}」`)}
			>チャンネル詳細</button
		>
		<button class="indent" onclick={() => copy(t.contact_url)}>コンタクト URL</button>
		<button class="indent" onclick={() => copy(pls)}>プレイリスト URL (pls)</button>
		<button class="indent" onclick={() => copy(t.id)}>channel ID</button>
		<button class="indent" onclick={() => copy(t.tip)}>配信元 IP (TIP)</button>
	</div>
{/if}

<style>
	main {
		display: grid;
		grid-template-rows: auto auto auto 1fr auto;
		min-height: 100vh;
		background: #fff;
		color: #1a1a1a;
		font-size: 12px;
		font-family:
			'Yu Gothic UI',
			Meiryo,
			-apple-system,
			BlinkMacSystemFont,
			sans-serif;
	}

	.toolbar {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.35rem 0.6rem;
		background: #f4f4f4;
		border-bottom: 1px solid #d0d0d0;
	}

	.toolbar button {
		padding: 0.2rem 0.7rem;
		font: inherit;
		background: #fff;
		border: 1px solid #b0b0b0;
		border-radius: 3px;
		cursor: pointer;
	}

	.toolbar button:hover {
		background: #f0f7ff;
	}

	.toolbar button:disabled {
		opacity: 0.5;
		cursor: progress;
	}

	.filter {
		flex: 1;
		padding: 0.2rem 0.5rem;
		font: inherit;
		border: 1px solid #b0b0b0;
		border-radius: 3px;
		background: #fff;
	}

	.stat {
		color: #666;
	}

	.updated {
		color: #888;
		font-size: 11px;
	}

	.tabs {
		display: flex;
		gap: 1px;
		padding: 0 0.4rem;
		background: #ececec;
		border-bottom: 1px solid #d0d0d0;
	}

	.tabs button {
		padding: 0.3rem 0.9rem;
		font: inherit;
		background: #ececec;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		color: #444;
	}

	.tabs button.active {
		background: #fff;
		color: #000;
		font-weight: 600;
		border-bottom-color: #46a3ff;
	}

	.error {
		padding: 0.4rem 0.6rem;
		background: #fff0f0;
		color: #c0392b;
		border-bottom: 1px solid #f0c4c4;
	}

	.warn {
		padding: 0.3rem 0.6rem;
		background: #fff8e0;
		color: #7a5d00;
		font-size: 11px;
		border-bottom: 1px solid #f0e0a0;
		display: flex;
		gap: 0.5rem;
		flex-wrap: wrap;
	}

	.failure {
		background: #fff;
		border: 1px solid #f0c060;
		border-radius: 2px;
		padding: 0 0.4rem;
	}

	.table-wrap {
		overflow: auto;
	}

	table {
		width: 100%;
		border-collapse: collapse;
	}

	thead th {
		background: #e6e6e6;
		text-align: left;
		font-weight: 600;
		padding: 0.25rem 0.5rem;
		border-bottom: 1px solid #c0c0c0;
		border-right: 1px solid #d8d8d8;
		position: sticky;
		top: 0;
		cursor: pointer;
		user-select: none;
		white-space: nowrap;
	}

	thead th:hover {
		background: #e0e8f0;
	}

	tbody td {
		padding: 0.18rem 0.5rem;
		border-bottom: 1px solid #ededed;
		border-right: 1px solid #f3f3f3;
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}

	tbody tr {
		cursor: default;
	}

	tbody tr:hover {
		filter: brightness(0.95);
	}

	tbody tr.selected {
		outline: 2px solid #7faaff;
		background: #cce0ff !important;
		color: #000 !important;
	}

	tbody tr.pinned {
		font-weight: 600;
	}

	tbody tr.newish td.col-name::before {
		content: '🆕 ';
		font-size: 10px;
		opacity: 0.8;
	}

	.col-name {
		max-width: 200px;
	}

	.col-desc {
		max-width: 520px;
	}

	.col-num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.col-uptime {
		font-variant-numeric: tabular-nums;
	}

	.col-yp {
		text-align: center;
		min-width: 40px;
	}

	.col-contact {
		max-width: 280px;
		color: #0a4cad;
	}

	.col-filter {
		min-width: 100px;
		color: #555;
	}

	.star {
		color: #ff8a3d;
		margin-right: 0.2rem;
	}

	.watching-badge {
		color: #2c7;
		margin-left: 0.3rem;
		font-weight: 600;
	}

	.empty {
		text-align: center;
		color: #888;
		padding: 1rem;
	}

	.statusbar {
		display: flex;
		gap: 0.5rem;
		padding: 0.3rem 0.6rem;
		background: #f0f0f0;
		border-top: 1px solid #d0d0d0;
		font-size: 11px;
		color: #444;
	}

	.statusbar .sep {
		color: #aaa;
	}

	.menu {
		position: fixed;
		background: #fff;
		border: 1px solid #777;
		border-radius: 3px;
		min-width: 240px;
		box-shadow: 0 4px 12px rgba(0, 0, 0, 0.2);
		padding: 4px 0;
		z-index: 1000;
		font-size: 12px;
	}

	.menu button {
		display: block;
		width: 100%;
		text-align: left;
		padding: 0.25rem 0.7rem;
		background: transparent;
		border: none;
		font: inherit;
		cursor: pointer;
		color: #1a1a1a;
	}

	.menu button:hover {
		background: #e0eaff;
	}

	.menu button.primary {
		font-weight: 600;
	}

	.menu button.indent {
		padding-left: 1.6rem;
	}

	.menu hr {
		border: none;
		border-top: 1px solid #ddd;
		margin: 4px 0;
	}

	.submenu-label {
		padding: 0.2rem 0.7rem;
		color: #555;
		font-size: 11px;
		background: #f8f8f8;
	}
</style>
