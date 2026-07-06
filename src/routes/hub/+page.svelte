<script lang="ts">
	// PSTPlayer Desktop ハブ画面 (PeCaRecorder 風)。
	// pstplayer を URL 引数なしで起動した時のメイン画面で、
	// YP テーブル + お気に入り適用 + 行クリックで別プロセス視聴起動を行う。
	//
	// 設計は docs/design/pstplayer-hub-*.svg, pstplayer-hub-interactions.md,
	// pstplayer-hub-settings.md を参照。

	import { onDestroy, onMount } from 'svelte';
	import { initTheme } from '$lib/theme';
	import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
	import { getCurrentWindow } from '@tauri-apps/api/window';
	import {
		CommandError,
		closeAllViewers,
		closeViewer,
		effectiveBackground,
		fetchYpSources,
		getConfig,
		isTauri,
		listActiveViewers,
		matchYpEntry,
		peercastPing,
		serverRecordList,
		serverRecordStart,
		serverRecordStop,
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
	let sortKey = $state<SortKey>(loadSortKey('hub.sortKey', 'listeners'));
	let sortDesc = $state(loadBool('hub.sortDesc', true));
	let activeTab = $state<TabKey>(loadTabKey('hub.activeTab', 'all'));
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

	// ── カラム幅 (手動リサイズ) ──────────────────────────────────
	// 末尾の contact (コンタクト) 列だけを可変幅にして残りを吸収させ、
	// それ以外の列は固定幅でヘッダー右端のハンドルをドラッグして変更
	// できる。各列の右境界ハンドル = その列をリサイズ (= 直感に一致) で、
	// 差分は flex の contact が吸収する。table-layout:fixed + width:100%
	// なのでウィンドウ幅への自動フィットも維持される。
	type ColId = 'name' | 'desc' | 'listeners' | 'bitrate' | 'uptime' | 'type' | 'filter' | 'yp';
	const COL_DEFAULTS: Record<ColId, number> = {
		name: 160,
		desc: 360,
		listeners: 72,
		bitrate: 56,
		uptime: 64,
		type: 48,
		filter: 96,
		yp: 44,
	};
	const COL_MIN = 32;
	let colW = $state<Record<ColId, number>>(loadColW());

	function loadColW(): Record<ColId, number> {
		try {
			const raw = localStorage.getItem('hub.colW');
			if (raw) {
				const parsed = JSON.parse(raw) as Partial<Record<ColId, number>>;
				const out = { ...COL_DEFAULTS };
				for (const k of Object.keys(COL_DEFAULTS) as ColId[]) {
					const v = parsed[k];
					if (typeof v === 'number' && v >= COL_MIN && v < 2000) out[k] = v;
				}
				return out;
			}
		} catch {
			/* ignore */
		}
		return { ...COL_DEFAULTS };
	}

	let resizing: { id: ColId; startX: number; startW: number } | null = null;

	function startColResize(e: MouseEvent, id: ColId) {
		e.preventDefault();
		e.stopPropagation();
		resizing = { id, startX: e.clientX, startW: colW[id] };
		window.addEventListener('mousemove', onColResizeMove);
		window.addEventListener('mouseup', onColResizeUp);
	}

	function onColResizeMove(e: MouseEvent) {
		if (!resizing) return;
		const w = Math.max(COL_MIN, resizing.startW + (e.clientX - resizing.startX));
		colW = { ...colW, [resizing.id]: w };
	}

	function onColResizeUp() {
		resizing = null;
		window.removeEventListener('mousemove', onColResizeMove);
		window.removeEventListener('mouseup', onColResizeUp);
		try {
			localStorage.setItem('hub.colW', JSON.stringify(colW));
		} catch {
			/* ignore */
		}
	}

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

	// localStorage に旧バージョン / 改竄で invalid な値が入っていても
	// default にフォールバックする (`as` キャストの type 不変条件を保つ)。
	function loadSortKey(key: string, defaultVal: SortKey): SortKey {
		const valid: SortKey[] = ['name', 'genre', 'listeners', 'bitrate', 'uptime', 'yp_source'];
		const v = loadStr(key, defaultVal);
		return (valid as string[]).includes(v) ? (v as SortKey) : defaultVal;
	}

	function loadTabKey(key: string, defaultVal: TabKey): TabKey {
		const v = loadStr(key, defaultVal);
		const builtin: TabKey[] = ['all', 'favorites', 'recording', 'watching', 'new'];
		if ((builtin as string[]).includes(v)) return v as TabKey;
		// 動的 YP タブは "yp:" prefix のみ許容 (実在チェックは render 側で)
		if (v.startsWith('yp:')) return v as TabKey;
		return defaultVal;
	}

	// 前回 fetch 時に見えていた channel_id の集合 (新着判定用)。
	let prevIds = $state<Set<string>>(new Set());
	let newIds = $state<Set<string>>(new Set());
	let watchingIds = $state<Set<string>>(new Set());
	let recordingIds = $state<Set<string>>(new Set());
	// 既に通知済みの新着 ID。重複通知防止 (同じセッションで何度も
	// 「新着 X」を出さない)。長時間運用で肥大化しないよう、追加時に
	// 1000 件で切る (FIFO に近い)。
	let notifiedIds = new Set<string>();
	const NOTIFIED_IDS_CAP = 1000;
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
	let closeUnlisten: UnlistenFn | null = null;
	let resizeUnlisten: UnlistenFn | null = null;
	// 閉じる確定後の destroy() で onCloseRequested が再入しないようにするフラグ。
	let closing = false;
	// 最小化トレイ設定 (起動時に config から固定。lib.rs のトレイ生成も起動時
	// 固定なので、トレイ無しでウィンドウが隠れる事故を避けられる)。
	let minimizeToTray = false;

	onMount(() => {
		const unbindTheme = initTheme();
		void refresh();
		void refreshWatching();
		// 設定ダイアログで保存があったら再 fetch (YP / お気に入りが変わる
		// 可能性があるので)。間隔も再計算する。Tauri イベントはブラウザには
		// 無い (呼ぶと transformCallback 例外で onMount が死ぬ。実機 QA)。
		if (isTauri()) {
			void listen('config:saved', () => {
				void refresh();
			}).then((u) => {
				configSavedUnlisten = u;
			});
		}
		// 録画中にメインウィンドウを閉じようとしたら確認する (実機 QA 要望)。
		// 録画は内蔵 / 外部 pst-server で進行するので、停止してファイルを正しく
		// クローズしてから終了する。ブラウザ (非 Tauri) では無効。
		if (isTauri()) {
			const win = getCurrentWindow();
			void win
				.onCloseRequested(async (event) => {
					if (closing) return;
					event.preventDefault(); // まず必ず止めてから判定する
					let recording = recordingIds.size > 0;
					if (pstServerUrl) {
						try {
							recording = (await serverRecordList(pstServerUrl)).length > 0;
						} catch {
							/* 取得不可なら recordingIds の値で判断する */
						}
					}
					if (recording) {
						const ok = confirm('録画中です。閉じると録画を停止します。閉じますか?');
						if (!ok) return; // 閉じない
						try {
							if (pstServerUrl) await serverRecordStop(pstServerUrl);
						} catch {
							/* 停止に失敗しても終了は続行する */
						}
					}
					closing = true;
					await win.destroy();
				})
				.then((u) => {
					closeUnlisten = u;
				});
			// 最小化トレイ: 設定 ON のとき最小化でウィンドウを隠す (トレイのみ)。
			// 値は起動時に固定する (lib.rs のトレイ生成も起動時固定)。
			void getConfig()
				.then((cfg) => {
					minimizeToTray = cfg?.window?.minimize_to_tray ?? false;
				})
				.catch(() => undefined);
			void win
				.onResized(async () => {
					if (!minimizeToTray) return;
					try {
						if (await win.isMinimized()) await win.hide();
					} catch {
						/* ignore */
					}
				})
				.then((u) => {
					resizeUnlisten = u;
				});
		}
		return () => {
			unbindTheme();
			if (refreshTimer) clearInterval(refreshTimer);
			if (watchingTimer) clearInterval(watchingTimer);
		};
	});

	onDestroy(() => {
		configSavedUnlisten?.();
		closeUnlisten?.();
		resizeUnlisten?.();
	});

	async function refreshWatching() {
		try {
			const ids = await listActiveViewers();
			watchingIds = new Set(ids);
			// 録画は常に pst-server が担当する。録画中バッジ / 「録画中」タブは
			// pst-server の録画一覧だけを正とする。pstServerUrl 未設定や
			// pst-server 未起動なら空 (黙って次回再試行)。
			if (pstServerUrl) {
				try {
					const server = await serverRecordList(pstServerUrl);
					recordingIds = new Set(server.map((r) => r.channel_id));
				} catch {
					recordingIds = new Set();
				}
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
			// 初回は必ず restartTimers() を通すこと。「値が変わったときだけ」
			// にすると、config が既定値 (60/5) と同じ場合に初期値と一致して
			// タイマーが一度も起動せず、YP 自動更新も「視聴中」ポーリングも
			// 死んだままになる (実機 QA: 最終更新が起動時刻のまま固着し、
			// 閉じた視聴ウィンドウが「視聴中 (1)」に残り続けた)。
			const timersNotStarted = refreshTimer === null && watchingTimer === null;
			if (timersNotStarted || newRefresh !== refreshSec || newPoll !== watchingPollSec) {
				refreshSec = newRefresh;
				watchingPollSec = newPoll;
				restartTimers();
			}
			dblClickAction = cfg?.hub?.double_click ?? 'watch';
			middleClickAction = cfg?.hub?.middle_click ?? 'open_bbs';
			pstServerUrl = cfg?.hub?.pst_server_url ?? '';
			// localStorage から復元された YP タブが、現 ypSources に
			// 存在しない (= ユーザが YP を削除した) 場合は 'all' に戻す
			if (activeTab.startsWith('yp:')) {
				const wanted = activeTab.slice(3);
				if (!ypSources.some((s) => s.name === wanted)) activeTab = 'all';
			}
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
					// 上限超過時は古いものから 1 件削除 (Set は挿入順を保つ)
					if (notifiedIds.size > NOTIFIED_IDS_CAP) {
						const first = notifiedIds.values().next().value;
						if (first !== undefined) notifiedIds.delete(first);
					}
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
		return matchYpEntry(favorites, e);
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
				// Ignore はメインタブ群 (すべて/お気に入り/新着/録画中/視聴中) では
				// 非表示。YP 個別タブだけは「その YP の生一覧」を見たい時のために
				// 表示する。
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

	// 選択中チャンネルのコンタクト URL (フッター表示用)。
	const selectedContact = $derived(visible.find((v) => v.e.id === selectedId)?.e.contact_url ?? '');

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
			// YP 個別タブのカウントは ignore も含める (その YP の生一覧として
			// 見せるため。メインタブのカウントには含めない)。
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
		// 録画は常に pst-server が担当する。「視聴 + 録画」は viewer(libmpv) で
		// 視聴しつつ pst-server にも録画を依頼する。ローカル PeerCast 本体へは
		// viewer + pst-server の 2 接続になるが、本体 → インターネットのリレーは
		// 1 本なので外向き帯域は増えない。録画には pst-server が必須。
		if (record && !pstServerUrl) {
			lastError =
				'「視聴 + 録画」の録画は pst-server が担当します (設定 → ハブ → pst-server URL を設定し、pst-server を起動してください)。録画なしの「視聴」はそのまま使えます。';
			return;
		}
		try {
			if (isTauri()) {
				// ネイティブ: 別プロセスの libmpv 視聴ウィンドウを起動。
				await spawnViewer(e.id, { tip: e.tip });
			} else {
				// ブラウザ: HLS 視聴ページを別タブで開く (/player?id=...)。
				// 未リレーのチャンネルは tip が無いと上流 PeerCast が join
				// できない (実測 503) ため、YP の tip を引き継ぐ。
				const tip = e.tip ? `&tip=${encodeURIComponent(e.tip)}` : '';
				window.open(`/player?id=${encodeURIComponent(e.id)}${tip}`, '_blank', 'noopener');
			}
			if (record) {
				await serverRecordStart(pstServerUrl, e.id, e.name ?? '');
			}
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

	async function stopRecordingRow(e: YpEntry) {
		closeMenu();
		// 録画は常に pst-server が担当するので停止も pst-server へ。
		if (!pstServerUrl) {
			lastError =
				'録画は pst-server が担当します (設定 → ハブ → pst-server URL を確認してください)。';
			return;
		}
		try {
			await serverRecordStop(pstServerUrl, e.id);
			setTimeout(() => {
				void refreshWatching();
			}, 400);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	// 「録画のみ」= 視聴ウィンドウを開かず pst-server に録画させる (再生なし /
	// 音なし)。録画は pst-server が HTTP ストリームを直接ファイルへ保存する。
	async function recordOnlyRow(e: YpEntry) {
		closeMenu();
		if (!pstServerUrl) {
			lastError =
				'「録画のみ」には pst-server が必要です (設定 → ハブ → pst-server URL を設定し、pst-server を起動してください)。「視聴 + 録画」なら不要です。';
			return;
		}
		try {
			await serverRecordStart(pstServerUrl, e.id, e.name ?? '', e.tip);
			setTimeout(() => {
				void refreshWatching();
			}, 600);
		} catch (err) {
			lastError = err instanceof Error ? err.message : String(err);
		}
	}

	async function startRecordingRow(e: YpEntry) {
		closeMenu();
		// 視聴中のまま録画開始。録画は pst-server が担当し、viewer はそのまま再生。
		if (!pstServerUrl) {
			lastError =
				'録画は pst-server が担当します (設定 → ハブ → pst-server URL を設定し、pst-server を起動してください)。';
			return;
		}
		try {
			await serverRecordStart(pstServerUrl, e.id, e.name ?? '', e.tip);
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
			'視聴したい PeerCast URL または channel_id (32 hex) を入力\n例: http://localhost:7144/pls/abc... / play.html?id=...',
		)?.trim();
		if (!url) return;
		// 1. 生 channel_id (32 hex) を許容
		// 2. URL から /pls/{id} / /stream/{id} / ?id={id} のいずれかで抽出
		let id: string | null = null;
		if (/^[0-9a-fA-F]{32}$/.test(url)) {
			id = url.toLowerCase();
		} else {
			const m =
				url.match(/\/(?:pls|stream|play\.html\?id=)\/?([0-9a-fA-F]{32})/) ??
				url.match(/[?&]id=([0-9a-fA-F]{32})/);
			id = m?.[1]?.toLowerCase() ?? null;
		}
		if (!id) {
			lastError = `URL から channel_id (32 hex) を抽出できません: ${url}`;
			return;
		}
		// URL に `?tip=host:port` が含まれていれば一緒に渡す。手入力でも
		// 自分の PeerCast が未 subscribe なら引き込み発火が必要なので
		// (YP 経由起動と同じ理由)。バックエンド側で安全性は検証される。
		const tipMatch = url.match(/[?&]tip=([^&]+)/);
		const tip = tipMatch?.[1] ? decodeURIComponent(tipMatch[1]) : undefined;
		try {
			await spawnViewer(id, { tip });
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
		// Tauri (デスクトップ) では OS ネイティブメニューで出す。HTML の
		// メニューはウィンドウ内にしか描けず、小さいウィンドウでは物理的に
		// 収まらない (実機 QA)。ネイティブならウィンドウ外にはみ出せる
		// (動画メニューの 0ba6499 と同じ方式)。ブラウザ (pst-server) は
		// HTML のまま、実測サイズでクランプする。
		if (isTauri()) {
			void showRowNativeMenu(e);
			return;
		}
		menuTarget = e;
		menuX = ev.clientX;
		menuY = ev.clientY;
		menuOpen = true;
	}

	// HTML メニュー (ブラウザ用フォールバック) の見切れ補正。開いた直後に
	// 実サイズを測って右端 / 下端からはみ出す分を押し戻す。
	let menuEl = $state<HTMLDivElement | null>(null);
	$effect(() => {
		if (!menuOpen || !menuEl) return;
		const r = menuEl.getBoundingClientRect();
		const maxX = Math.max(0, window.innerWidth - r.width - 8);
		const maxY = Math.max(0, window.innerHeight - r.height - 8);
		if (menuX > maxX) menuX = maxX;
		if (menuY > maxY) menuY = maxY;
	});

	// OS ネイティブの行コンテキストメニュー (Tauri のみ)。
	async function showRowNativeMenu(t: YpEntry) {
		const { Menu, MenuItem, PredefinedMenuItem, Submenu } = await import('@tauri-apps/api/menu');
		const sep = () => PredefinedMenuItem.new({ item: 'Separator' });
		const items = [];
		if (watchingIds.has(t.id)) {
			items.push(
				await MenuItem.new({
					text: '✕ 視聴ウィンドウを閉じる',
					action: () => void closeRow(t),
				}),
			);
			if (recordingIds.has(t.id)) {
				items.push(
					await MenuItem.new({ text: '⏹ 録画停止', action: () => void stopRecordingRow(t) }),
				);
			} else {
				items.push(
					await MenuItem.new({
						text: '⏺ 録画開始 (視聴中のまま)',
						action: () => void startRecordingRow(t),
					}),
				);
			}
		} else {
			items.push(
				await MenuItem.new({
					text: '▶ 視聴 (別ウィンドウで開く)',
					action: () => void watchRow(t),
				}),
			);
			if (recordingIds.has(t.id)) {
				// 「録画のみ」中 (視聴ウィンドウ無し) でも停止できること。
				items.push(
					await MenuItem.new({ text: '⏹ 録画停止', action: () => void stopRecordingRow(t) }),
				);
			} else {
				items.push(
					await MenuItem.new({ text: '⏺ 視聴 + 録画開始', action: () => void watchRow(t, true) }),
				);
				items.push(
					await MenuItem.new({
						text: '⏺ 録画のみ (ウィンドウ無し)',
						action: () => void recordOnlyRow(t),
					}),
				);
			}
		}
		items.push(await sep());
		items.push(
			await MenuItem.new({
				text: '📺 BBS としてコンタクト URL を開く',
				enabled: !!t.contact_url,
				action: () => void openBbs(t.contact_url),
			}),
		);
		items.push(
			await MenuItem.new({
				text: '🌐 コンタクト URL をブラウザで開く',
				enabled: !!t.contact_url,
				action: () => openInBrowser(t.contact_url),
			}),
		);
		items.push(await sep());
		const favItems = await Promise.all(
			favorites.map((rule, i) =>
				MenuItem.new({
					text: rule.name || '(無名ルール)',
					action: () => void appendToFavorite(rule, i, t.name),
				}),
			),
		);
		favItems.push(
			await MenuItem.new({
				text: '＋ 新規ルールとして追加…',
				action: () => void addToFavorite(t.name),
			}),
		);
		items.push(await Submenu.new({ text: '★ お気に入りに追加', items: favItems }));
		const pls = plsUrlFor(t, currentPeerHost, currentPeerPort);
		const copyItems = await Promise.all([
			MenuItem.new({ text: 'チャンネル名', action: () => void copy(t.name) }),
			MenuItem.new({
				text: 'チャンネル詳細',
				action: () => void copy(`[${t.genre}] ${t.desc} 「${t.comment}」`),
			}),
			MenuItem.new({ text: 'コンタクト URL', action: () => void copy(t.contact_url) }),
			MenuItem.new({ text: 'プレイリスト URL (pls)', action: () => void copy(pls) }),
			MenuItem.new({ text: 'channel ID', action: () => void copy(t.id) }),
			MenuItem.new({ text: '配信元 IP (TIP)', action: () => void copy(t.tip) }),
		]);
		items.push(await Submenu.new({ text: '📋 コピー', items: copyItems }));
		const menu = await Menu.new({ items });
		await menu.popup();
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
		if (isTauri()) {
			// tauri-plugin-opener で OS の既定ブラウザに渡す。
			void import('@tauri-apps/plugin-opener').then((m) => m.openUrl(url)).catch(() => undefined);
		} else {
			// ブラウザ: 別タブで開く。
			window.open(url, '_blank', 'noopener');
		}
		closeMenu();
	}

	let pstServerUrl = $state('');

	// 設定を開く。Tauri は専用ウィンドウ、ブラウザは /settings を別タブで。
	async function openSettingsUi() {
		if (isTauri()) {
			await openSettings();
		} else {
			window.open('/settings', '_blank', 'noopener');
		}
	}

	async function openBbs(url: string) {
		if (!url) return;
		if (isTauri()) {
			await openThreadList(url);
		} else {
			// ブラウザ: コンタクト URL (BBS) を別タブで開く。視聴ページ (/player) にも
			// BBS ペインがあるが、ここはハブからの「BBS を開く」操作の素直な対応。
			window.open(url, '_blank', 'noopener');
		}
		closeMenu();
	}

	// 設定ウィンドウが新規作成の場合、設定側のリスナー登録 (onMount) より
	// 先に emit すると捨てられる (実機 QA: 設定が開くだけでルールが追加
	// されない)。ack が返るまで同じ token で再送し、設定側は token で重複
	// 適用を防ぐ。
	async function emitToSettingsWithAck(event: string, payload: Record<string, unknown>) {
		const token = `${Date.now()}-${Math.random().toString(36).slice(2)}`;
		let acked = false;
		const un = await listen<{ token?: string }>(`${event}:ack`, (ev) => {
			if (ev.payload?.token === token) acked = true;
		});
		try {
			for (let i = 0; i < 20 && !acked; i++) {
				await emit(event, { ...payload, token });
				await new Promise((r) => setTimeout(r, 250));
			}
		} finally {
			un();
		}
	}

	async function addToFavorite(channelName: string) {
		closeMenu();
		await openSettingsUi();
		await emitToSettingsWithAck('settings:add-favorite', { channelName });
	}

	// 既存のお気に入りルールに、このチャンネル名を `既存|チャンネル名` の形で
	// 追記する (お気に入りマッチは `|` 区切りで OR)。設定のお気に入りタブへ
	// 遷移し、保存ボタンで確定する。
	async function appendToFavorite(rule: FavoriteRule, index: number, channelName: string) {
		closeMenu();
		await openSettingsUi();
		await emitToSettingsWithAck('settings:append-favorite', {
			index,
			name: rule.name,
			channelName,
		});
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
		} else if ((ev.ctrlKey || ev.metaKey) && /^[1-9]$/.test(ev.key)) {
			ev.preventDefault();
			const n = Number(ev.key);
			const builtin: TabKey[] = ['all', 'favorites', 'new', 'recording', 'watching'];
			const ypTabs: TabKey[] = ypSources
				.filter((s) => s.show_tab)
				.map((s) => `yp:${s.name}` as TabKey);
			const allTabs = [...builtin, ...ypTabs];
			if (n - 1 < allTabs.length) activeTab = allTabs[n - 1];
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
			// DOM 更新後にスクロール。setTimeout(0) で次マイクロタスクへ遅延。
			setTimeout(() => {
				document
					.querySelector<HTMLElement>('tr.selected')
					?.scrollIntoView({ block: 'nearest', behavior: 'instant' });
			}, 0);
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
		<button onclick={refresh} disabled={loading}
			>↻<span class="btn-label">{loading ? ' 更新中…' : ' 更新'}</span></button
		>
		<button class="hide-mobile" onclick={watchUrl} title="URL を直接入力して視聴する"
			>🔗 URL から開く</button
		>
		<button onclick={openSettingsUi}>⚙<span class="btn-label"> 設定</span></button>
		<button
			class="hide-mobile"
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
	</header>

	<nav class="tabs">
		<button class:active={activeTab === 'all'} onclick={() => (activeTab = 'all')}>
			すべて ({counts.all})
		</button>
		<button class:active={activeTab === 'favorites'} onclick={() => (activeTab = 'favorites')}>
			お気に入り ({counts.fav})
		</button>
		<button
			class="hide-mobile"
			class:active={activeTab === 'new'}
			onclick={() => (activeTab = 'new')}
		>
			新着 ({counts.fresh})
		</button>
		<button
			class="hide-mobile"
			class:active={activeTab === 'recording'}
			onclick={() => (activeTab = 'recording')}
		>
			● 録画中 ({counts.recording})
		</button>
		<button
			class="hide-mobile"
			class:active={activeTab === 'watching'}
			onclick={() => (activeTab = 'watching')}
		>
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
		<details class="warn">
			<summary>⚠ YP 取得失敗 ({failures.length} 件) — 詳細を表示</summary>
			<ul>
				{#each failures as f}
					<li><strong>[{f.source}]</strong> {f.url}<br /><small>{f.error}</small></li>
				{/each}
			</ul>
		</details>
	{/if}

	<div class="table-wrap">
		<table>
			<colgroup>
				<col class="col-play-col" style:width={'34px'} />
				<col style:width={colW.name + 'px'} />
				<col style:width={colW.desc + 'px'} />
				<col style:width={colW.listeners + 'px'} />
				<col style:width={colW.bitrate + 'px'} />
				<col style:width={colW.uptime + 'px'} />
				<col style:width={colW.type + 'px'} />
				<col style:width={colW.filter + 'px'} />
				<col style:width={colW.yp + 'px'} />
				<!-- contact は可変幅 (残りを吸収) -->
				<col />
			</colgroup>
			<thead>
				<tr>
					<th class="col-play" aria-label="再生"></th>
					<th class="col-name" onclick={() => toggleSort('name')}
						>チャンネル名{arrow('name')}<span
							class="col-resizer"
							role="separator"
							aria-label="チャンネル名の幅を変更"
							onmousedown={(e) => startColResize(e, 'name')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-desc" onclick={() => toggleSort('genre')}
						>ジャンル - 詳細 「コメント」{arrow('genre')}<span
							class="col-resizer"
							role="separator"
							aria-label="詳細の幅を変更"
							onmousedown={(e) => startColResize(e, 'desc')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-num" onclick={() => toggleSort('listeners')}
						>👤{arrow('listeners')}<span
							class="col-resizer"
							role="separator"
							aria-label="リスナー数の幅を変更"
							onmousedown={(e) => startColResize(e, 'listeners')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-num" onclick={() => toggleSort('bitrate')}
						>kbps{arrow('bitrate')}<span
							class="col-resizer"
							role="separator"
							aria-label="kbps の幅を変更"
							onmousedown={(e) => startColResize(e, 'bitrate')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-uptime" onclick={() => toggleSort('uptime')}
						>配信{arrow('uptime')}<span
							class="col-resizer"
							role="separator"
							aria-label="配信時間の幅を変更"
							onmousedown={(e) => startColResize(e, 'uptime')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-type"
						>形式<span
							class="col-resizer"
							role="separator"
							aria-label="形式の幅を変更"
							onmousedown={(e) => startColResize(e, 'type')}
						></span></th
					>
					<th class="col-filter"
						>フィルタ<span
							class="col-resizer"
							role="separator"
							aria-label="フィルタの幅を変更"
							onmousedown={(e) => startColResize(e, 'filter')}
						></span></th
					>
					<th class="col-yp" onclick={() => toggleSort('yp_source')}
						>YP{arrow('yp_source')}<span
							class="col-resizer"
							role="separator"
							aria-label="YP の幅を変更"
							onmousedown={(e) => startColResize(e, 'yp')}
							onclick={(e) => e.stopPropagation()}
						></span></th
					>
					<th class="col-contact">コンタクト</th>
				</tr>
			</thead>
			<tbody>
				{#each visible as { e, rule } (e.id + '@' + e.yp_source)}
					{@const src = ypSources.find((s) => s.name === e.yp_source)}
					{@const bg = effectiveBackground(rule) || src?.background || ''}
					{@const fg = rule?.text_color || src?.text_color || ''}
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
						class:rule-colored={!!fg}
						style:background={bg || undefined}
						style:color={fg || undefined}
						title={tip}
						onclick={() => onRowClick(e)}
						ondblclick={() => onRowDblClick(e)}
						onmousedown={(ev) => onRowMouseDown(ev, e)}
						oncontextmenu={(ev) => onRowContextMenu(ev, e)}
					>
						<td class="col-play">
							<button
								class="play-btn"
								title="このチャンネルを再生"
								aria-label="再生"
								onclick={(ev) => {
									ev.stopPropagation();
									void watchRow(e);
								}}
								ondblclick={(ev) => ev.stopPropagation()}>▶</button
							>
						</td>
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
						<td class="col-num col-listeners">{e.listeners} / {e.relays}</td>
						<td class="col-num col-bitrate">{e.bitrate}</td>
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
								<button onclick={openSettingsUi} class="empty-cta">⚙ 設定で YP を追加する</button>
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
		<span>PeerCast: {currentPeerHost}:{currentPeerPort}</span>
		{#if selectedContact}
			<span class="sep">·</span>
			<span class="contact" title={selectedContact}>{selectedContact}</span>
		{/if}
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
		bind:this={menuEl}
		style:left={menuX + 'px'}
		style:top={menuY + 'px'}
		onclick={(ev) => ev.stopPropagation()}
		role="menu"
		tabindex="-1"
	>
		{#if watchingIds.has(t.id)}
			<button onclick={() => closeRow(t)} class="primary">✕ 視聴ウィンドウを閉じる</button>
			{#if recordingIds.has(t.id)}
				<button onclick={() => stopRecordingRow(t)}>⏹ 録画停止</button>
			{:else}
				<button onclick={() => startRecordingRow(t)}>⏺ 録画開始 (視聴中のまま)</button>
			{/if}
		{:else}
			<button onclick={() => watchRow(t)} class="primary">▶ 視聴 (別ウィンドウで開く)</button>
			{#if recordingIds.has(t.id)}
				<button onclick={() => stopRecordingRow(t)}>⏹ 録画停止</button>
			{:else}
				<button onclick={() => watchRow(t, true)}>⏺ 視聴 + 録画開始</button>
				<button
					onclick={() => recordOnlyRow(t)}
					title="pst-server に録画させる (視聴ウィンドウを開かず、再生も音も無し)"
					>⏺ 録画のみ</button
				>
			{/if}
		{/if}
		<hr />
		<button onclick={() => openBbs(t.contact_url)} disabled={!t.contact_url}
			>📺 BBS としてコンタクト URL を開く</button
		>
		<button onclick={() => openInBrowser(t.contact_url)} disabled={!t.contact_url}
			>🌐 コンタクト URL をブラウザで開く</button
		>
		{#if isTauri()}
			<!-- お気に入り編集は設定ウィンドウ (Tauri 専用) が担うため、
			     ブラウザのハブではセクションごと出さない。 -->
			<hr />
			<div class="submenu-label">★ お気に入りに追加</div>
			{#each favorites as rule, i (i)}
				<button
					class="indent"
					onclick={() => appendToFavorite(rule, i, t.name)}
					title={`「${rule.pattern || rule.channel_name}」に「${t.name}」を追記`}
					>{rule.name || '(無名ルール)'}</button
				>
			{/each}
			<button class="indent" onclick={() => addToFavorite(t.name)}>＋ 新規ルールとして追加…</button>
		{/if}
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
	/* body のデフォルトマージン等で外側 (ウィンドウ) にスクロールバーが
	   出てしまうのを防ぐ。スクロールは内側の一覧 (.table-wrap) だけにする。 */
	:global(html, body) {
		margin: 0;
		height: 100%;
		overflow: hidden;
	}

	main {
		/* flex column + .table-wrap flex:1 でフッターを常にウィンドウ下部に
		   固定する。以前は grid 5 トラック固定だったが、エラー行 (.error) と
		   失敗詳細 (.warn) が条件付きで増減して行数がずれ、フッターが 1fr
		   トラックに乗り、項目が少ないとき下部固定されず上に詰まっていた。
		   ビューポート固定高さ + overflow:hidden で一覧だけ内部スクロール。 */
		display: flex;
		flex-direction: column;
		/* iOS Safari のアドレスバー対応 (100vh だと下端が隠れる)。 */
		height: 100dvh;
		overflow: hidden;
		background: var(--bg);
		color: var(--fg);
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
		background: var(--bg-elev);
		border-bottom: 1px solid var(--border);
	}

	.toolbar button {
		padding: 0.2rem 0.7rem;
		font: inherit;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		cursor: pointer;
	}

	.toolbar button:hover {
		background: color-mix(in srgb, var(--accent) 12%, var(--bg-input));
	}

	.toolbar button:disabled {
		opacity: 0.5;
		cursor: progress;
	}

	.filter {
		flex: 1;
		padding: 0.2rem 0.5rem;
		font: inherit;
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		background: var(--bg-input);
		color: inherit;
	}

	.tabs {
		display: flex;
		gap: 1px;
		padding: 0 0.4rem;
		background: var(--bg);
		border-bottom: 1px solid var(--border);
		overflow-x: auto;
		scrollbar-width: thin;
		white-space: nowrap;
	}

	.tabs button {
		flex: 0 0 auto;
		white-space: nowrap;
		padding: 0.3rem 0.9rem;
		font: inherit;
		background: transparent;
		border: none;
		border-bottom: 2px solid transparent;
		cursor: pointer;
		color: var(--fg-dim);
	}

	.tabs button.active {
		background: var(--bg-elev);
		color: var(--fg);
		font-weight: 600;
		border-bottom-color: var(--accent);
	}

	.error {
		padding: 0.4rem 0.6rem;
		background: color-mix(in srgb, var(--err) 14%, var(--bg));
		color: var(--err);
		border-bottom: 1px solid color-mix(in srgb, var(--err) 35%, var(--bg));
	}

	.warn {
		padding: 0.3rem 0.6rem;
		background: color-mix(in srgb, var(--accent-external) 12%, var(--bg));
		color: var(--accent-external);
		font-size: 11px;
		border-bottom: 1px solid color-mix(in srgb, var(--accent-external) 35%, var(--bg));
	}

	.warn summary {
		cursor: pointer;
		font-weight: 600;
	}

	.warn ul {
		margin: 0.4rem 0 0;
		padding-left: 1.2rem;
	}

	.warn li {
		margin: 0.2rem 0;
	}

	.warn small {
		color: var(--accent-external);
	}

	.table-wrap {
		flex: 1;
		overflow: auto;
		/* grid の 1fr セルを縮められるようにして内部スクロールを有効化。
		   thead th は position:sticky;top:0 でカラムヘッダーも追従固定。 */
		min-height: 0;
	}

	table {
		width: 100%;
		border-collapse: collapse;
		/* colgroup の col 幅を権威にして手動リサイズを効かせる。desc 列は
		   幅指定なし (auto) なので残り幅を吸収し、ウィンドウ幅への自動
		   フィットも維持される。 */
		table-layout: fixed;
	}

	thead th {
		background: var(--bg-elev);
		text-align: left;
		font-weight: 600;
		padding: 0.25rem 0.5rem;
		border-bottom: 1px solid var(--border-strong);
		border-right: 1px solid var(--border);
		position: sticky;
		top: 0;
		/* z-index を付けないと、スクロール中に tbody のセル (チャンネル名等)
		   が sticky ヘッダーの上に描画されて透けて見える。ヘッダーを常に
		   前面に固定する。 */
		z-index: 3;
		cursor: pointer;
		user-select: none;
		white-space: nowrap;
	}

	thead th:hover {
		background: color-mix(in srgb, var(--accent) 10%, var(--bg-elev));
	}

	tbody td {
		padding: 0.18rem 0.5rem;
		border-bottom: 1px solid color-mix(in srgb, var(--border) 55%, var(--bg));
		border-right: 1px solid color-mix(in srgb, var(--border) 40%, var(--bg));
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
		outline: 2px solid var(--accent);
		background: color-mix(in srgb, var(--accent) 28%, var(--bg)) !important;
		color: var(--fg) !important;
	}

	tbody tr.pinned {
		font-weight: 600;
	}

	tbody tr.newish td.col-name::before {
		content: '🆕 ';
		font-size: 10px;
		opacity: 0.8;
	}

	/* 列幅は colgroup (table-layout:fixed) で制御するので max-width は不要。
	   ここでは表示属性 (寄せ / 色) だけ指定する。 */
	.col-num {
		text-align: right;
		font-variant-numeric: tabular-nums;
	}

	.col-uptime {
		font-variant-numeric: tabular-nums;
	}

	.col-yp {
		text-align: center;
	}

	.col-contact {
		color: var(--accent-link);
	}

	.col-filter {
		color: var(--fg-muted);
	}

	/* カラム幅変更用ハンドル。ヘッダー右端に重ねる。 */
	.col-resizer {
		position: absolute;
		top: 0;
		right: 0;
		width: 6px;
		height: 100%;
		cursor: col-resize;
		user-select: none;
		z-index: 4;
	}
	.col-resizer:hover {
		background: var(--accent);
	}

	.play-btn {
		border: 1px solid #56617a;
		background: transparent;
		color: inherit;
		border-radius: 3px;
		font-size: 10px;
		line-height: 1;
		padding: 2px 5px;
		margin-right: 4px;
		cursor: pointer;
		vertical-align: middle;
	}
	.play-btn:hover {
		background: #56617a;
		color: #fff;
	}
	/* スマホ幅: デスクトップ向け操作ボタンを隠し、YP 一覧はカード型の
	   1 カラム表示にする (再生ボタン | チャンネル名 + ジャンル-詳細 |
	   右端に 視聴数 / 配信時間 / kbps の3段)。ソートはチャンネル名のみ。
	   input の font-size は 16px 未満だと iOS Safari がフォーカス時に
	   自動ズームするため 16px 以上にする。 */
	@media (max-width: 700px) {
		.hide-mobile {
			display: none;
		}
		main {
			font-size: 15px;
		}
		.filter {
			font-size: 16px;
		}
		/* 更新 / 設定はアイコンのみ。枠は保ちつつ中の文字を大きく。 */
		.toolbar button {
			font-size: 30px;
			line-height: 1.2;
			padding: 0;
			min-width: 44px;
		}
		.btn-label {
			display: none;
		}

		table,
		tbody {
			display: block;
		}
		colgroup {
			display: none;
		}
		thead,
		thead tr {
			display: block;
			position: sticky;
			top: 0;
			z-index: 3;
		}
		thead th {
			display: none;
		}
		thead th.col-name {
			display: block;
			padding: 8px 10px;
			border-right: none;
			position: static;
		}
		.col-resizer {
			display: none;
		}

		tbody tr {
			display: grid;
			grid-template-columns: 48px minmax(0, 1fr) max-content;
			grid-template-rows: auto auto auto;
			grid-template-areas:
				'play name stat1'
				'play desc stat2'
				'play desc stat3';
			column-gap: 10px;
			padding: 8px 10px;
			border-bottom: 1px solid var(--border);
			align-items: center;
		}
		tbody td {
			display: block;
			padding: 0;
			border: none;
			overflow: hidden;
		}
		td.col-play {
			grid-area: play;
			justify-self: center;
		}
		td.col-name {
			grid-area: name;
			font-size: 16px;
			font-weight: 600;
			white-space: nowrap;
			text-overflow: ellipsis;
		}
		td.col-desc {
			grid-area: desc;
			align-self: start;
			font-size: 13px;
			color: var(--fg-dim);
			white-space: normal;
			display: -webkit-box;
			-webkit-line-clamp: 2;
			line-clamp: 2;
			-webkit-box-orient: vertical;
		}
		td.col-listeners {
			grid-area: stat1;
		}
		td.col-uptime {
			grid-area: stat2;
		}
		td.col-bitrate {
			grid-area: stat3;
		}
		td.col-listeners,
		td.col-uptime,
		td.col-bitrate {
			font-size: 11px;
			color: var(--fg-dim);
			text-align: right;
			white-space: nowrap;
		}
		/* フィルタ (お気に入りルール) の文字色指定がある行はそれを優先。 */
		tr.rule-colored td.col-desc,
		tr.rule-colored td.col-listeners,
		tr.rule-colored td.col-uptime,
		tr.rule-colored td.col-bitrate {
			color: inherit;
		}
		td.col-type,
		td.col-filter,
		td.col-yp,
		td.col-contact {
			display: none;
		}
		.play-btn {
			font-size: 16px;
			padding: 10px 12px;
			margin: 0;
		}
	}
	td.col-play {
		text-align: center;
		padding: 0 2px;
	}
	.star {
		color: #ff8a3d;
		margin-right: 0.2rem;
	}

	.watching-badge {
		color: var(--accent-name);
		margin-left: 0.3rem;
		font-weight: 600;
	}

	.recording-badge {
		color: var(--err);
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
		color: var(--fg-muted);
		padding: 1rem;
	}

	.empty .empty-cta {
		margin-top: 0.5rem;
		padding: 0.3rem 0.8rem;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		cursor: pointer;
		font: inherit;
	}

	.empty .empty-cta:hover {
		background: color-mix(in srgb, var(--accent) 12%, var(--bg-input));
	}

	.statusbar {
		display: flex;
		gap: 0.5rem;
		padding: 0.3rem 0.6rem;
		background: var(--bg-elev);
		border-top: 1px solid var(--border);
		font-size: 11px;
		color: var(--fg-dim);
	}

	.statusbar .contact {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
		min-width: 0;
	}
	.statusbar .sep {
		color: var(--fg-muted);
	}

	.statusbar .filler {
		flex: 1;
	}

	.statusbar .muted {
		color: var(--fg-muted);
	}

	.statusbar .loading-indicator {
		color: var(--accent);
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
		background: var(--bg-elev);
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		min-width: 240px;
		/* お気に入りが多いとメニューが長くなるのでスクロール可能にする。 */
		max-height: 80vh;
		overflow-y: auto;
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
		color: var(--fg);
	}

	.menu button:hover {
		background: color-mix(in srgb, var(--accent) 15%, var(--bg-elev));
	}

	.menu button.primary {
		font-weight: 600;
	}

	.menu button.indent {
		padding-left: 1.6rem;
	}

	.menu hr {
		border: none;
		border-top: 1px solid var(--border);
		margin: 4px 0;
	}

	.submenu-label {
		padding: 0.2rem 0.7rem;
		color: var(--fg-muted);
		font-size: 11px;
		background: var(--bg);
	}
</style>
