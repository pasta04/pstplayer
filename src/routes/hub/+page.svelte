<script lang="ts">
	// PSTPlayer Desktop ハブ画面 (PeCaRecorder 風)。
	// pstplayer を URL 引数なしで起動した時のメイン画面で、
	// YP テーブル + お気に入り適用 + 行クリックで別プロセス視聴起動を行う。
	//
	// 設計は docs/design/pstplayer-hub-*.svg, pstplayer-hub-interactions.md,
	// pstplayer-hub-settings.md を参照。

	import { onDestroy, onMount } from 'svelte';
	import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
	import {
		CommandError,
		closeAllViewers,
		closeViewer,
		effectiveBackground,
		fetchYpSources,
		firstFavoriteMatch,
		getConfig,
		listActiveViewers,
		listRecordingViewers,
		peercastPing,
		spawnViewer,
		type FavoriteAction,
		type FavoriteRule,
		type HubClickAction,
		type YpEntry,
		type YpFetchFailure,
		type YpSource,
	} from '$lib/api';
	import { openSettings, openThreadList } from '$lib/windows';
	import { notify } from '$lib/notifications';

	type SortKey = 'name' | 'genre' | 'listeners' | 'bitrate' | 'uptime' | 'yp_source';
	type BuiltinTab = 'all' | 'favorites' | 'recording' | 'watching' | 'new';
	// 動的タブは "yp:<source_name>" の prefix で識別。
	type TabKey = BuiltinTab | `yp:${string}`;

	let entries = $state<YpEntry[]>([]);
	let failures = $state<YpFetchFailure[]>([]);
	let favorites = $state<FavoriteRule[]>([]);
	let ypSources = $state<YpSource[]>([]);
	let loading = $state(false);
	let lastError = $state<string | null>(null);
	let lastUpdatedAt = $state<Date | null>(null);

	let filter = $state(loadStr('hub.filter', ''));
	let sortKey = $state<SortKey>(loadStr('hub.sortKey', 'listeners') as SortKey);
	let sortDesc = $state(loadBool('hub.sortDesc', true));
	let activeTab = $state<TabKey>(loadStr('hub.activeTab', 'all') as TabKey);
	let selectedId = $state<string | null>(null);

	// localStorage への永続化 (ソート / タブ / フィルタはセッション跨ぎ)。
	$effect(() => {
		try {
			localStorage.setItem('hub.filter', filter);
			localStorage.setItem('hub.sortKey', sortKey);
			localStorage.setItem('hub.sortDesc', sortDesc ? '1' : '0');
			localStorage.setItem('hub.activeTab', activeTab);
		} catch {
			/* ignore: 容量超過 / private mode */
		}
	});

	function loadStr(key: string, defaultVal: string): string {
		try {
			return localStorage.getItem(key) ?? defaultVal;
		} catch {
			return defaultVal;
		}
	}

	function loadBool(key: string, defaultVal: boolean): boolean {
		try {
			const v = localStorage.getItem(key);
			if (v === null) return defaultVal;
			return v === '1';
		} catch {
			return defaultVal;
		}
	}

	// 前回 fetch 時に見えていた channel_id の集合 (新着判定用)。
	let prevIds = $state<Set<string>>(new Set());
	let newIds = $state<Set<string>>(new Set());
	let watchingIds = $state<Set<string>>(new Set());
	let recordingIds = $state<Set<string>>(new Set());
	// 既に通知済みの新着 ID。重複通知防止 (同じセッションで何度も
	// 「新着 X」を出さない)。
	let notifiedIds = new Set<string>();
	let firstRefreshDone = false;

	// 自動再 fetch / 視聴中ポーリングの間隔 (秒)。config から読む。
	// 0 で無効。
	let refreshSec = 60;
	let watchingPollSec = 5;
	let dblClickAction = $state<HubClickAction>('watch');
	let middleClickAction = $state<HubClickAction>('open_bbs');
	let refreshTimer: ReturnType<typeof setInterval> | null = null;
	let watchingTimer: ReturnType<typeof setInterval> | null = null;

	function restartTimers() {
		if (refreshTimer) clearInterval(refreshTimer);
		if (watchingTimer) clearInterval(watchingTimer);
		refreshTimer = null;
		watchingTimer = null;
		if (refreshSec > 0) {
			refreshTimer = setInterval(() => {
				void refresh();
			}, refreshSec * 1000);
		}
		if (watchingPollSec > 0) {
			watchingTimer = setInterval(() => {
				void refreshWatching();
			}, watchingPollSec * 1000);
		}
	}

	let menuOpen = $state(false);
	let menuX = $state(0);
	let menuY = $state(0);
	let menuTarget = $state<YpEntry | null>(null);

	let configSavedUnlisten: UnlistenFn | null = null;

	onMount(() => {
		void refresh();
		void refreshWatching();
		// 設定ダイアログで保存があったら再 fetch (YP / お気に入りが変わる
		// 可能性があるので)。間隔も再計算する。
		void listen('config:saved', () => {
			void refresh();
		}).then((u) => {
			configSavedUnlisten = u;
		});
		return () => {
			if (refreshTimer) clearInterval(refreshTimer);
			if (watchingTimer) clearInterval(watchingTimer);
		};
	});

	onDestroy(() => {
		configSavedUnlisten?.();
	});

	async function refreshWatching() {
		try {
			const ids = await listActiveViewers();
			watchingIds = new Set(ids);
			// 録画中チェック (各 viewer に IPC 投げる)。視聴中 0 件なら
			// 録画中も 0 件なので呼び出し省略。
			if (ids.length > 0) {
				const recIds = await listRecordingViewers();
				recordingIds = new Set(recIds);
			} else {
				recordingIds = new Set();
			}
		} catch {
			/* ignore: lock 読み取りエラーは無視して次回再試行 */
		}
	}

	async function refresh() {
		if (loading) return; // 並行 refresh 防止 (前回完了前に次が走らない)
		loading = true;
		lastError = null;
		try {
			const cfg = await getConfig();
			favorites = cfg?.favorites?.rules ?? [];
			ypSources = cfg?.yp?.sources ?? [];
			const newRefresh = cfg?.hub?.refresh_sec ?? 60;
			const newPoll = cfg?.hub?.watching_poll_sec ?? 5;
			if (newRefresh !== refreshSec || newPoll !== watchingPollSec) {
				refreshSec = newRefresh;
				watchingPollSec = newPoll;
				restartTimers();
			}
			dblClickAction = cfg?.hub?.double_click ?? 'watch';
			middleClickAction = cfg?.hub?.middle_click ?? 'open_bbs';
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
				if (activeTab === 'recording' && !recordingIds.has(e.id)) return false;
				if (activeTab === 'watching' && !watchingIds.has(e.id)) return false;
				if (activeTab.startsWith('yp:')) {
					const wanted = activeTab.slice(3);
					if (e.yp_source !== wanted) return false;
				} else if (activeTab === 'all') {
					// 「すべて」タブで show_in_all=false の YP は非表示
					const src = ypSources.find((s) => s.name === e.yp_source);
					if (src && !src.show_in_all) return false;
				}
				// Ignore はすべて/お気に入り/新着では非表示にする (専用タブ無いので一旦隠すだけ)
				if (actionOf(rule) === 'ignore' && !activeTab.startsWith('yp:')) return false;
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
			watching = 0,
			recording = 0;
		const perYp = new Map<string, number>();
		for (const e of entries) {
			const rule = matchFor(e);
			if (actionOf(rule) === 'block') continue;
			// YP 別カウントは ignore でも数える (専用タブなら表示するため)
			perYp.set(e.yp_source, (perYp.get(e.yp_source) ?? 0) + 1);
			if (actionOf(rule) === 'ignore') continue;
			const src = ypSources.find((s) => s.name === e.yp_source);
			if (src && !src.show_in_all) {
				// 「すべて」からは外す。カウントには含めない
			} else {
				all++;
				if (rule) fav++;
				if (newIds.has(e.id)) fresh++;
				if (watchingIds.has(e.id)) watching++;
				if (recordingIds.has(e.id)) recording++;
			}
		}
		return { all, fav, fresh, watching, recording, perYp };
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

	async function watchRow(e: YpEntry, record = false) {
		closeMenu();
		try {
			await spawnViewer(e.id, { record });
			// 即座に「視聴中」リストを更新 (5 秒待たずにバッジが付く)。
			// spawn 直後は lock が完了していないかもしれないので少し待つ。
			setTimeout(() => {
				void refreshWatching();
			}, 800);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	async function closeRow(e: YpEntry) {
		closeMenu();
		try {
			await closeViewer(e.id);
			setTimeout(() => {
				void refreshWatching();
			}, 400);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	async function closeAll() {
		try {
			const n = await closeAllViewers();
			setTimeout(() => {
				void refreshWatching();
			}, 400);
			if (n === 0) lastError = '視聴中のウィンドウはありません';
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	async function watchUrl() {
		const url = prompt(
			'視聴したい PeerCast URL を入力 (例: http://localhost:7144/pls/abc...)',
		)?.trim();
		if (!url) return;
		// URL から channel_id を抽出 (簡易: 末尾セグメント)
		const match = url.match(/\/(?:pls|stream)\/([0-9a-fA-F]{32})/);
		if (!match) {
			lastError = `URL から channel_id を抽出できません: ${url}`;
			return;
		}
		try {
			await spawnViewer(match[1]);
			setTimeout(() => {
				void refreshWatching();
			}, 800);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	function onRowClick(e: YpEntry) {
		selectedId = e.id;
	}

	function performAction(action: HubClickAction, e: YpEntry) {
		switch (action) {
			case 'watch':
				void watchRow(e);
				break;
			case 'watch_and_record':
				void watchRow(e, true);
				break;
			case 'open_bbs':
				void openBbs(e.contact_url);
				break;
			case 'open_contact':
				openInBrowser(e.contact_url);
				break;
			case 'none':
			default:
				break;
		}
	}

	function onRowDblClick(e: YpEntry) {
		performAction(dblClickAction, e);
	}

	function onRowMouseDown(ev: MouseEvent, e: YpEntry) {
		// マウス中ボタン (button === 1) を判定。click イベントだと
		// auxclick が必要だがここでは mousedown で簡易ハンドル。
		if (ev.button === 1) {
			ev.preventDefault();
			performAction(middleClickAction, e);
		}
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

	async function addToFavorite(channelName: string) {
		closeMenu();
		await openSettings();
		// 設定ウィンドウが既に開いていても、新規でも、emit は届く。
		await emit('settings:add-favorite', { channelName });
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
			if (e) {
				// Shift+Enter で「視聴 + 録画」、通常 Enter で「視聴」
				if (ev.shiftKey) void watchRow(e, true);
				else void watchRow(e);
			}
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
		<button onclick={watchUrl} title="URL を直接入力して視聴する">🔗 URL から開く</button>
		<button onclick={openSettings}>⚙ 設定</button>
		<button
			onclick={closeAll}
			disabled={watchingIds.size === 0}
			title="開いている全視聴ウィンドウを閉じる"
		>
			✕ 全閉じ ({watchingIds.size})
		</button>
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
			● 録画中 ({counts.recording})
		</button>
		<button class:active={activeTab === 'watching'} onclick={() => (activeTab = 'watching')}>
			視聴中 ({counts.watching})
		</button>
		{#each ypSources.filter((s) => s.show_tab) as src (src.name)}
			{@const key = `yp:${src.name}` as TabKey}
			<button
				class:active={activeTab === key}
				onclick={() => (activeTab = key)}
				style:border-bottom-color={activeTab === key ? src.background || '#46a3ff' : 'transparent'}
			>
				{src.name} ({counts.perYp.get(src.name) ?? 0})
			</button>
		{/each}
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
					{@const tip = [
						e.name,
						e.genre ? `[${e.genre}]` : '',
						e.desc ?? '',
						e.comment ? `「${e.comment}」` : '',
						rule ? `★ ${rule.name || 'お気に入り'}` : '',
						`👤 ${e.listeners} / ${e.relays} · ${e.bitrate} kbps · ${e.uptime}`,
						`YP: ${e.yp_source}`,
						`ID: ${e.id}`,
						`TIP: ${e.tip}`,
					]
						.filter(Boolean)
						.join('\n')}
					<tr
						class:selected={selectedId === e.id}
						class:pinned={rule?.pin_top}
						class:newish={newIds.has(e.id)}
						style:background={bg || undefined}
						style:color={fg || undefined}
						title={tip}
						onclick={() => onRowClick(e)}
						ondblclick={() => onRowDblClick(e)}
						onmousedown={(ev) => onRowMouseDown(ev, e)}
						oncontextmenu={(ev) => onRowContextMenu(ev, e)}
					>
						<td class="col-name">
							{#if rule}<span class="star">★</span>{/if}{e.name}{#if watchingIds.has(e.id)}
								<span class="watching-badge" title="このチャンネルは視聴ウィンドウで開いています"
									>▶</span
								>{/if}{#if recordingIds.has(e.id)}
								<span class="recording-badge" title="このチャンネルは録画中です">●</span>
							{/if}
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
							{#if loading}
								読み込み中…
							{:else if ypSources.length === 0}
								<div>YP が登録されていません。</div>
								<button onclick={openSettings} class="empty-cta">⚙ 設定で YP を追加する</button>
							{:else if entries.length === 0 && failures.length > 0}
								<div>全 YP の取得に失敗しています。</div>
								<button onclick={refresh} class="empty-cta">↻ 再試行</button>
							{:else}
								チャンネルがありません (絞り込み / タブの設定を確認してください)
							{/if}
						</td>
					</tr>
				{/if}
			</tbody>
		</table>
	</div>

	<footer class="statusbar">
		<span>★ {counts.fav} / 全 {counts.all} ch</span>
		<span class="sep">·</span>
		<span>視聴中 {counts.watching}</span>
		<span class="sep">·</span>
		<span>YP {ypSources.length} 件{failures.length > 0 ? ` (失敗 ${failures.length})` : ''}</span>
		<span class="sep">·</span>
		<span>PeerCast: {currentPeerHost}:{currentPeerPort}</span>
		<span class="filler"></span>
		{#if loading}
			<span class="loading-indicator">⟳ 更新中…</span>
		{:else if lastUpdatedAt}
			<span class="muted">最終更新: {fmtTime(lastUpdatedAt)}</span>
		{/if}
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
		{#if watchingIds.has(t.id)}
			<button onclick={() => closeRow(t)} class="primary">✕ 視聴ウィンドウを閉じる</button>
		{:else}
			<button onclick={() => watchRow(t)} class="primary">▶ 視聴 (別ウィンドウで開く)</button>
			<button onclick={() => watchRow(t, true)}>⏺ 視聴 + 録画開始</button>
		{/if}
		<hr />
		<button onclick={() => openBbs(t.contact_url)} disabled={!t.contact_url}
			>📺 BBS としてコンタクト URL を開く</button
		>
		<button onclick={() => openInBrowser(t.contact_url)} disabled={!t.contact_url}
			>🌐 コンタクト URL をブラウザで開く</button
		>
		<hr />
		<button onclick={() => addToFavorite(t.name)}>★ お気に入りルールに追加…</button>
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
		overflow-x: auto;
		scrollbar-width: thin;
		white-space: nowrap;
	}

	.tabs button {
		flex: 0 0 auto;
		white-space: nowrap;
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

	.recording-badge {
		color: #c0392b;
		margin-left: 0.3rem;
		font-weight: 600;
		animation: pulse 1.6s ease-in-out infinite;
	}

	@keyframes pulse {
		0%,
		100% {
			opacity: 1;
		}
		50% {
			opacity: 0.4;
		}
	}

	.empty {
		text-align: center;
		color: #888;
		padding: 1rem;
	}

	.empty .empty-cta {
		margin-top: 0.5rem;
		padding: 0.3rem 0.8rem;
		background: var(--bg-elev, #fff);
		border: 1px solid #bbb;
		border-radius: 3px;
		cursor: pointer;
		font: inherit;
	}

	.empty .empty-cta:hover {
		background: #f0f7ff;
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

	.statusbar .filler {
		flex: 1;
	}

	.statusbar .muted {
		color: #888;
	}

	.statusbar .loading-indicator {
		color: #0a4cad;
		animation: spin 1s linear infinite;
		display: inline-block;
	}

	@keyframes spin {
		from {
			transform: rotate(0);
		}
		to {
			transform: rotate(360deg);
		}
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
