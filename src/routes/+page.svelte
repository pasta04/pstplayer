<script lang="ts">
	import { onDestroy, onMount, tick } from 'svelte';
	import { emit, listen, type UnlistenFn } from '@tauri-apps/api/event';
	import {
		CommandError,
		bumpChannel,
		boardUrlOf,
		threadUrlOf,
		fetchBoardSetting,
		endpointForUrl,
		fetchChannelInfo,
		fetchChannelStatus,
		fetchThread,
		firstFavoriteMatch,
		getCliArgs,
		getConfig,
		getHistory,
		listThreads,
		playerAttach,
		playerSetVideoRect,
		playerLoad,
		playerRecordPath,
		playerRecordStart,
		playerRecordStop,
		playerSetAspect,
		playerSetAutoReconnect,
		playerSetVolume,
		playerSnapshot,
		playerStatus,
		playerStop,
		postToThread,
		pushHistory,
		resolveStreamUrl,
		sanitizeHtml,
		setConfig,
		startChannelPolling,
		stopChannel,
		stopChannelPolling,
		type PlayerStatus,
		type ChannelInfo,
		type ChannelStatus,
		type FavoriteRule,
		type FetchState,
		type HistoryEntry,
		type PeerCastEndpoint,
		type Post,
		type SubjectEntry,
	} from '$lib/api';
	import { formatUptime, linkifySanitized, renderBodyHtml, renderIdHtml } from '$lib/format';
	import { openSettings, openYpList } from '$lib/windows';
	import { installShortcuts, setAlwaysOnTop, setDecorations } from '$lib/shortcuts';
	import { notify } from '$lib/notifications';
	import { initTheme } from '$lib/theme';
	import { restoreMainWindowGeometry, watchMainWindowGeometry } from '$lib/window-state';

	// ── State ────────────────────────────────────────────────────────

	let pasteUrl = $state('');
	let busy = $state(false);
	let lastError = $state<string | null>(null);
	// libmpv 描画用子ウィンドウを重ねる対象の DOM 要素。streamUrl がある
	// 時だけ存在する。位置 / サイズの変化を ResizeObserver で監視して
	// バックエンドの子ウィンドウに反映する (player_set_video_rect)。
	let playerCanvasEl = $state<HTMLDivElement | null>(null);
	let videoRectObserver: ResizeObserver | null = null;
	// 自動再接続の進行 / 結果を表示するメッセージ。null なら非表示。
	// バックエンドからの `player:reconnecting` / `player:reconnect_stopped`
	// イベントで更新される。詳細は src-tauri/src/player/engine.rs。
	let reconnectStatus = $state<string | null>(null);
	let reconnectStatusTimer: ReturnType<typeof setTimeout> | null = null;

	// ── Display toggles (T/Z/X/B/C shortcuts + 表示 menu) ────────────
	let showBbsPane = $state(true);
	let showStatusBar = $state(true);
	let showTitleBar = $state(true); // titlebar text visibility (decorations stay)
	let showFrame = $state(true); // window decorations
	let alwaysOnTop = $state(false);

	// BBS ペインの幅 (px)。プレイヤーとの境界スプリッターをドラッグして変更し
	// localStorage に保存する (#14)。
	const BBS_W_MIN = 180;
	const BBS_W_MAX = 900;
	let bbsWidth = $state(loadBbsWidth());
	let bbsResize: { startX: number; startW: number } | null = null;

	function loadBbsWidth(): number {
		try {
			const v = Number(localStorage.getItem('pst.bbsWidth'));
			if (Number.isFinite(v) && v >= BBS_W_MIN && v <= BBS_W_MAX) return v;
		} catch {
			/* ignore */
		}
		return 320;
	}
	function startBbsResize(e: MouseEvent) {
		e.preventDefault();
		bbsResize = { startX: e.clientX, startW: bbsWidth };
		window.addEventListener('mousemove', onBbsResizeMove);
		window.addEventListener('mouseup', onBbsResizeUp);
	}
	function onBbsResizeMove(e: MouseEvent) {
		if (!bbsResize) return;
		// BBS は右側なので、スプリッターを左へドラッグ (clientX 減) で幅が増える。
		const w = bbsResize.startW - (e.clientX - bbsResize.startX);
		bbsWidth = Math.min(BBS_W_MAX, Math.max(BBS_W_MIN, w));
	}
	function onBbsResizeUp() {
		bbsResize = null;
		window.removeEventListener('mousemove', onBbsResizeMove);
		window.removeEventListener('mouseup', onBbsResizeUp);
		try {
			localStorage.setItem('pst.bbsWidth', String(Math.round(bbsWidth)));
		} catch {
			/* ignore */
		}
	}

	let streamUrl = $state<string | null>(null);
	let endpoint = $state<PeerCastEndpoint | null>(null);
	let channelId = $state<string | null>(null);
	let channelInfo = $state<ChannelInfo | null>(null);
	let channelStatus = $state<ChannelStatus | null>(null);
	// channelStatus は backend が ~5 秒間隔で更新する。稼働時間 (uptime) を
	// 1 秒刻みで進めるため、最後に status を受け取った時刻と現在時刻を保持し、
	// 経過分を加算して表示する (statusAtMs / nowMs)。
	let statusAtMs = $state(0);
	let nowMs = $state(Date.now());

	let threadList = $state<SubjectEntry[]>([]);
	let currentThreadUrl = $state<string | null>(null);
	let posts = $state<Post[]>([]);
	let fetchState = $state<FetchState | null>(null);
	let threadLoading = $state(false);
	// 「更新中…」インジケータ表示用。5 秒毎のサイレント自動更新では出さず、
	// 初回表示 / スレ切替 / 手動の全再取得 (forceReset) の時だけ出す。
	// 自動更新のたびに点滅すると鬱陶しい (実機 QA で発覚)。
	let threadReloading = $state(false);
	// dat / rawmode が 404 / 410 / DAT_NOT_FOUND 系を返したら、スレが
	// 落ちた (削除 or 過去ログ送り) と判定し以降の自動更新を止める。
	let threadDead = $state(false);
	// 自動スレ移動用: 現チャンネルの板トップ URL と、その板の最大レス数
	// (SETTING の BBS_THREAD_STOP / BBS_RES_MAX)。max が取れない時は
	// THREAD_FULL_FALLBACK を使う。
	let currentBoardUrl = $state<string | null>(null);
	let boardMaxRes = $state(0);
	const THREAD_FULL_FALLBACK = 1000;
	// 自動スレ移動中の再入防止。
	let advancingThread = false;
	// スレ一覧オーバーレイパネルの開閉 / 読み込み状態。
	let showThreadList = $state(false);
	let threadListLoading = $state(false);

	let writeName = $state('');
	let writeMail = $state('sage');
	// reloadBbsPrefs() の中で初回 mount 時だけ config から writeName /
	// writeMail を流し込むためのフラグ。$state ではないので Svelte の
	// reactivity には乗らない (= 単なる副作用フラグ)。
	let bbsPrefsInitialised = false;
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
	// 自動スクロールの速度 (px/秒)。新着時に一瞬で飛ばずスムーズに流して
	// 読めるようにするための値。設定 (bbs.autoscrollSpeed) から読む (#20)。
	let autoscrollSpeed = $state(600);
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
	// レス一覧 (.posts) の右クリックメニュー (C5)。動画メニュー (ctxMenu) と独立。
	let postsMenu = $state<{ x: number; y: number } | null>(null);
	// 視聴履歴 (右クリックメニューのサブメニュー用に config から都度取得)。
	let history = $state<HistoryEntry[]>([]);
	// チャンネル詳細モーダル (リレー / 接続情報の全フィールドを見るため)。
	let showChannelDetails = $state(false);
	// 録画中の保存先パス。null = 録画していない。
	let recordPath = $state<string | null>(null);

	// Volume (0-100). Wheel over the player area changes it.
	let volume = $state(80);

	// Seconds until the next BBS auto-refresh tick (5s cycle).
	const REFRESH_SEC = 5;
	let refreshCountdown = $state(REFRESH_SEC);

	// Polling handles (channel status は backend ポーラー経由なのでここ
	// では持たない)
	let threadTimer: ReturnType<typeof setInterval> | null = null;
	let playerTimer: ReturnType<typeof setInterval> | null = null;
	let countdownTimer: ReturnType<typeof setInterval> | null = null;
	let threadSelectedUnlisten: UnlistenFn | null = null;
	let configSavedUnlisten: UnlistenFn | null = null;
	let channelStatusUnlisten: UnlistenFn | null = null;
	let reconnectingUnlisten: UnlistenFn | null = null;
	let reconnectStoppedUnlisten: UnlistenFn | null = null;
	let endFileObservedUnlisten: UnlistenFn | null = null;

	let shortcutsUnlisten: (() => void) | null = null;
	let themeUnlisten: (() => void) | null = null;
	let windowGeomUnlisten: (() => void) | null = null;
	let focusUnlisten: UnlistenFn | null = null;

	// libmpv 描画用子ウィンドウを `.player-canvas` の物理ピクセル矩形へ
	// 合わせる。getBoundingClientRect() は CSS px・クライアント原点基準
	// なので devicePixelRatio を掛けて物理 px に直す (= 親ウィンドウの
	// クライアント座標 = SetWindowPos が期待する座標)。
	let videoRectRaf = 0;
	function syncVideoRect() {
		if (videoRectRaf) cancelAnimationFrame(videoRectRaf);
		videoRectRaf = requestAnimationFrame(() => {
			videoRectRaf = 0;
			const el = playerCanvasEl;
			if (!el) return;
			const r = el.getBoundingClientRect();
			const dpr = window.devicePixelRatio || 1;
			void playerSetVideoRect(
				Math.round(r.left * dpr),
				Math.round(r.top * dpr),
				Math.round(r.width * dpr),
				Math.round(r.height * dpr),
			).catch(() => {});
		});
	}

	// player-canvas が出現 / 消滅したら ResizeObserver を張り替える。
	// レイアウト変化 (BBS ペイン開閉・バー表示切替・全画面) はここで拾う。
	$effect(() => {
		videoRectObserver?.disconnect();
		const el = playerCanvasEl;
		if (!el) return;
		videoRectObserver = new ResizeObserver(() => syncVideoRect());
		videoRectObserver.observe(el);
		syncVideoRect();
	});

	onMount(async () => {
		themeUnlisten = initTheme();

		// Hand the main Tauri window to libmpv so it renders into our surface
		// (`wid` property). Best-effort: on Wayland this is unsupported and
		// the engine just stays detached, which is fine for headless / dev.
		playerAttach('main')
			.then(() => syncVideoRect())
			.catch((e) => {
				console.warn('player_attach failed (libmpv overlay disabled)', e);
			});

		// ウィンドウのリサイズ / DPI 変化で子ウィンドウを追従。レイアウト
		// 変化 (BBS ペイン開閉・バー表示切替・全画面) は player-canvas 要素の
		// ResizeObserver ($effect 内) が拾う。
		window.addEventListener('resize', syncVideoRect);

		// Restore last main window position/size, then start watching.
		await restoreMainWindowGeometry();
		windowGeomUnlisten = await watchMainWindowGeometry();

		// Load BBS display mode + submit key from config (best-effort).
		await reloadBbsPrefs();

		// Re-read prefs + hotkeys whenever the settings window saves.
		configSavedUnlisten = await listen('config:saved', () => {
			reloadBbsPrefs();
			reinstallShortcuts();
			syncAutoReconnect();
		});

		// 設定ウィンドウが別プロセス (hub から spawn された視聴ウィンドウ等) で
		// 開かれていると config:saved の emit が届かないことがある。ウィンドウに
		// フォーカスが戻ったタイミングで設定を読み直し、変更を取りこぼさない。
		try {
			const { getCurrentWindow } = await import('@tauri-apps/api/window');
			focusUnlisten = await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
				if (focused) reloadBbsPrefs();
			});
		} catch {
			/* best-effort */
		}

		threadSelectedUnlisten = await listen<{
			boardUrl: string;
			key: string;
			title: string;
		}>('thread:selected', async (e) => {
			// 板 URL + key から板の流儀に合ったスレ URL をバックエンドで
			// 構築する (`${base}/${key}/` の素朴連結は 2ch 互換で壊れる)。
			try {
				currentThreadUrl = await threadUrlOf(e.payload.boardUrl, e.payload.key);
			} catch {
				const base = e.payload.boardUrl.replace(/\/+$/, '');
				currentThreadUrl = `${base}/${e.payload.key}/`;
			}
			fetchState = null;
			posts = [];
			await loadCurrentThread(true);
		});

		// YP ウィンドウからのチャンネル選択は ADR-0006 Step 4 で別プロセス
		// (`pstplayer.exe <url>`) として spawn する形に変更したため、
		// ここでの listen は不要 (自プロセス内 emit は飛んでこない)。

		// バックエンドの pseudo-push (5 秒間隔でチャンネル状態を fetch
		// → channel:status event)。フロント側 setInterval を 1 箇所に
		// まとめる目的。複数ウィンドウからも同じ event を listen 可。
		channelStatusUnlisten = await listen<ChannelStatus>('channel:status', (e) => {
			channelStatus = e.payload;
			statusAtMs = Date.now();
		});

		// 自動再接続イベント (詳細は src-tauri/src/player/engine.rs)。
		// observation モード (config.player.auto_reconnect = false) でも
		// end_file_observed は飛んでくるので、reason 値の挙動確認に使える。
		endFileObservedUnlisten = await listen<{
			reason: string;
			auto_reconnect_enabled: boolean;
		}>('player:end_file_observed', (e) => {
			// 観察モード時のみ短く表示。有効時は reconnecting / stopped で
			// より詳細を表示するためここでは黙る。
			if (!e.payload.auto_reconnect_enabled) {
				showReconnectStatus(`配信終了/切断 (reason=${e.payload.reason})`, 8000);
			}
		});
		reconnectingUnlisten = await listen<{
			attempt: number;
			max: number;
			delay_sec: number;
			end_file_reason: string;
		}>('player:reconnecting', (e) => {
			const p = e.payload;
			showReconnectStatus(
				`自動再接続中… 試行 ${p.attempt}/${p.max} (${p.delay_sec}秒後 / reason=${p.end_file_reason})`,
				null, // 次のイベントまで表示し続ける
			);
		});
		reconnectStoppedUnlisten = await listen<{
			reason: string;
			end_file_reason: string;
		}>('player:reconnect_stopped', (e) => {
			const p = e.payload;
			const msg = reconnectStopMessage(p.reason, p.end_file_reason);
			if (msg) {
				showReconnectStatus(msg, 15000);
			} else {
				// user-stop / not-reconnectable / disabled は通常運用なので消すだけ。
				clearReconnectStatus();
			}
		});

		// Honour CLI args (positional URL → auto-play unless --no-autoplay).
		// CLI url が無ければ初期表示として:
		//   - PeerCast に応答あり → YP ウィンドウを自動オープン
		//   - PeerCast 未起動 / port 違い → 設定ウィンドウを開いて促す
		// CLI URL 引数なしで起動された場合はハブ画面 (PeCaRecorder 風)
		// に遷移する (ADR-0006 / pstplayer-hub-mockup.svg)。視聴ウィンドウは
		// ハブ画面から行クリックで別プロセスとして spawn する設計なので、
		// 既存の「貼り付け + 単一プレイヤー」UI は CLI URL があるときだけ
		// 使う。
		try {
			const cli = await getCliArgs();
			if (cli.url && !cli.no_autoplay) {
				pasteUrl = cli.url;
				await onPaste();
			} else if (!cli.no_autoplay) {
				const { goto } = await import('$app/navigation');
				await goto('/hub', { replaceState: true });
				return;
			}
		} catch {
			/* CLI parsing is best-effort */
		}

		await reinstallShortcuts();
	});

	async function reinstallShortcuts() {
		shortcutsUnlisten?.();
		let customs: Record<string, string> = {};
		try {
			const cfg = await getConfig();
			customs = (cfg?.hotkeys as Record<string, string>) ?? {};
		} catch {
			/* default bindings */
		}
		shortcutsUnlisten = installShortcuts(
			{
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
				toggleFullscreen: ctxToggleFullscreen,
				setSizePreset: applySizePreset,
				setAspectPreset: applyAspectPreset,
			},
			customs,
		);
	}

	onDestroy(() => {
		if (threadTimer) clearInterval(threadTimer);
		if (playerTimer) clearInterval(playerTimer);
		if (countdownTimer) clearInterval(countdownTimer);
		if (reconnectStatusTimer) clearTimeout(reconnectStatusTimer);
		threadSelectedUnlisten?.();
		configSavedUnlisten?.();
		focusUnlisten?.();
		channelStatusUnlisten?.();
		reconnectingUnlisten?.();
		reconnectStoppedUnlisten?.();
		endFileObservedUnlisten?.();
		shortcutsUnlisten?.();
		themeUnlisten?.();
		windowGeomUnlisten?.();
		videoRectObserver?.disconnect();
		window.removeEventListener('resize', syncVideoRect);
		if (videoRectRaf) cancelAnimationFrame(videoRectRaf);
		cancelSmoothScroll();
		stopChannelPolling().catch(() => undefined);
	});

	async function reloadBbsPrefs() {
		try {
			const cfg = await getConfig();
			displayMode = cfg?.bbs?.displayMode === 'html' ? 'html' : 'plain';
			submitKey = cfg?.bbs?.submitKey === 'shift_enter' ? 'shift_enter' : 'ctrl_enter';
			notifyOnNewPost = cfg?.bbs?.notifyOnNewPost === true;
			autoscroll = cfg?.bbs?.autoscroll !== false;
			{
				const sp = Number(cfg?.bbs?.autoscrollSpeed);
				autoscrollSpeed = Number.isFinite(sp) && sp > 0 ? sp : 600;
			}
			// 書き込み欄の名前 / メールは「初回 mount 時だけ」config から
			// 流し込む。config:saved やフォーカス復帰での再読込時は触らない
			// (ユーザが書きかけの値を消してしまう事故を防ぐため)。設定値が
			// 空のメールは sage 進行が PeerCast BBS の慣習に合うので 'sage'
			// で埋める。名前は空のままにして「名無しさん」扱いに委ねる。
			if (!bbsPrefsInitialised) {
				const cfgName = (cfg?.bbs?.defaultName ?? '').trim();
				const cfgMail = (cfg?.bbs?.defaultMail ?? '').trim();
				writeName = cfgName;
				writeMail = cfgMail || 'sage';
				bbsPrefsInitialised = true;
			}
		} catch {
			/* defaults */
		}
	}

	// 視聴ウィンドウ側から新着自動スクロールの ON/OFF を切り替える。即時に
	// 反映 (autoscroll 状態) しつつ config に永続化し、同プロセスの他ウィンドウ
	// (設定画面等) へ config:saved で伝える。別プロセスの視聴ウィンドウへは
	// フォーカス時の reloadBbsPrefs で伝わる。
	async function toggleAutoscroll() {
		autoscroll = !autoscroll;
		try {
			const cfg = await getConfig();
			if (cfg?.bbs) {
				cfg.bbs.autoscroll = autoscroll;
				await setConfig(cfg);
				await emit('config:saved');
			}
		} catch {
			/* best-effort: 状態だけは即時反映済み */
		}
	}

	// ── Post-list auto scroll ────────────────────────────────────────
	//
	// Spec (docs/ui-design.md §147): デフォルト ON。手動スクロール時は
	// 一時停止 = ユーザーが末尾付近にいない時は追従しない。
	const NEAR_BOTTOM_PX = 24;

	// ── 自動再接続ステータス表示 ─────────────────────────────────

	function showReconnectStatus(msg: string, autoHideMs: number | null) {
		reconnectStatus = msg;
		if (reconnectStatusTimer) clearTimeout(reconnectStatusTimer);
		if (autoHideMs !== null) {
			reconnectStatusTimer = setTimeout(() => {
				reconnectStatus = null;
				reconnectStatusTimer = null;
			}, autoHideMs);
		}
	}

	function clearReconnectStatus() {
		if (reconnectStatusTimer) clearTimeout(reconnectStatusTimer);
		reconnectStatusTimer = null;
		reconnectStatus = null;
	}

	/// 設定保存後に backend の auto_reconnect 値を即時同期する。
	async function syncAutoReconnect() {
		try {
			const cfg = await getConfig();
			const enabled = cfg?.player?.auto_reconnect === true;
			await playerSetAutoReconnect(enabled);
		} catch {
			// failed read or call: ignore (engine will catch on next load())
		}
	}

	/// Skip 理由 → 表示メッセージ。null を返す reason は通常運用なので
	/// ステータス帯に出さない (= ノイズを減らす)。
	function reconnectStopMessage(skipReason: string, endFileReason: string): string | null {
		switch (skipReason) {
			case 'user-stop':
			case 'not-reconnectable':
			case 'disabled':
				return null;
			case 'no-url':
				return null; // 内部状態の不整合、表示不要
			case 'max-attempts':
				return `自動再接続を停止しました (上限到達)。F5 で手動再接続できます。`;
			case 'total-timeout':
				return `自動再接続を停止しました (合計時間超過)。F5 で手動再接続できます。`;
			case 'broadcast-likely-ended':
				return `自動再接続を停止しました (配信終了の可能性、reason=${endFileReason})。`;
			default:
				return `自動再接続を停止しました (${skipReason})。`;
		}
	}

	function isNearBottom(el: HTMLElement | null): boolean {
		if (!el) return false;
		return el.scrollTop + el.clientHeight >= el.scrollHeight - NEAR_BOTTOM_PX;
	}

	function scrollPostsToBottom() {
		if (postsEl) postsEl.scrollTop = postsEl.scrollHeight;
	}

	// 新着レス到着時の自動スクロール (#20)。一瞬で飛ばず autoscrollSpeed
	// (px/秒) でスムーズに最下部へ流して読めるようにする。毎フレーム最下部を
	// 測り直すのでスクロール中にさらに新着が来ても追従する。前のアニメは
	// 取り消し、ユーザーが手動で上にスクロールしたら止める (cancelSmoothScroll)。
	let smoothScrollRaf = 0;
	function smoothScrollToBottom() {
		const el = postsEl;
		if (!el) return;
		if (smoothScrollRaf) cancelAnimationFrame(smoothScrollRaf);
		const speed = Math.max(50, autoscrollSpeed);
		let last = performance.now();
		const step = (now: number) => {
			const el2 = postsEl;
			if (!el2) {
				smoothScrollRaf = 0;
				return;
			}
			const dt = (now - last) / 1000;
			last = now;
			const target = el2.scrollHeight - el2.clientHeight;
			const remaining = target - el2.scrollTop;
			if (remaining <= 1) {
				el2.scrollTop = target;
				smoothScrollRaf = 0;
				return;
			}
			el2.scrollTop += Math.min(remaining, speed * dt);
			smoothScrollRaf = requestAnimationFrame(step);
		};
		smoothScrollRaf = requestAnimationFrame(step);
	}
	function cancelSmoothScroll() {
		if (smoothScrollRaf) {
			cancelAnimationFrame(smoothScrollRaf);
			smoothScrollRaf = 0;
		}
	}

	/// 初回ロード時の「最下部へ」。多数レスやサニタイズ後にレイアウト高さが
	/// 後から確定するため、tick 後に加えて次の 2 フレームでも測り直して
	/// 確実に最新レスを画面内に収める。
	async function scrollPostsToBottomSettled() {
		await tick();
		scrollPostsToBottom();
		requestAnimationFrame(() => {
			scrollPostsToBottom();
			requestAnimationFrame(() => scrollPostsToBottom());
		});
	}

	// ── Derived ──────────────────────────────────────────────────────

	const visiblePosts = $derived.by(() => {
		const raw = filter.trim();
		if (!raw) return posts;

		// `>>N` / `>>N-M` でレス番号抽出 (前後 3 件も含めて文脈が読める
		// ようにする)。
		const anchor = raw.match(/^(?:>>|＞＞)?(\d+)(?:-(\d+))?$/);
		if (anchor) {
			const from = Math.max(1, Number(anchor[1]) - 2);
			const to = anchor[2] ? Number(anchor[2]) + 2 : Number(anchor[1]) + 2;
			return posts.filter((p) => p.number >= from && p.number <= to);
		}

		// `id:xxx` で同一 ID 抽出 (大文字小文字無視、部分一致)。
		const idMatch = raw.match(/^id:(.+)$/i);
		if (idMatch) {
			const idQ = idMatch[1].trim().toLowerCase();
			if (idQ) return posts.filter((p) => p.id.toLowerCase().includes(idQ));
			return posts;
		}

		// それ以外はキーワード横断検索 (本文 / 名前 / ID / レス番号)。
		const q = raw.toLowerCase();
		return posts.filter(
			(p) =>
				p.body.toLowerCase().includes(q) ||
				p.name.toLowerCase().includes(q) ||
				p.id.toLowerCase().includes(q) ||
				String(p.number) === q,
		);
	});

	// 現在開いているスレッドのタイトル。スレッドバーに URL でなくこれを
	// 出す。1) 取得済みレスのスレタイ (通常 1 レス目)、2) スレ一覧から
	// 現スレ key で引いたタイトル、の順で探す。どちらも無ければ null。
	const currentThreadTitle = $derived.by(() => {
		const fromPost = posts.find((p) => p.threadTitle.trim())?.threadTitle.trim();
		if (fromPost) return fromPost;
		const key = currentThreadUrl?.match(/(\d+)\/?$/)?.[1];
		if (key) {
			const t = threadList.find((e) => e.key === key)?.title?.trim();
			if (t) return t;
		}
		return null;
	});

	const statusLine = $derived.by(() => {
		if (!channelInfo) return null;
		const name = channelInfo.name || '(unnamed)';
		const br = channelInfo.bitrate ? `${channelInfo.bitrate} kbps` : '-';
		const ldir = channelStatus ? `L:${channelStatus.localDirects}` : '';
		const lrel = channelStatus ? `R:${channelStatus.localRelays}` : '';
		const fps = playerStat?.fps && playerStat.fps > 0 ? `${playerStat.fps.toFixed(1)}fps` : '';
		const size =
			playerStat?.width && playerStat?.height ? `${playerStat.width}×${playerStat.height}` : '';
		return { name, br, ldir, lrel, fps, size };
	});

	// 稼働時間 (uptime) を 1 秒刻みで表示するためのライブ値。status 受信時刻
	// (statusAtMs) からの経過秒を backend の uptime に加算する。nowMs は 1 秒
	// タイマー (countdownTimer) で更新される。
	const liveUptimeSec = $derived(
		channelStatus ? channelStatus.uptime + Math.max(0, Math.floor((nowMs - statusAtMs) / 1000)) : 0,
	);

	// ウィンドウタイトル (= タスクバー表示) にチャンネル名を出す (#18)。複数
	// 配信を同時に開いたときにタスクバーで区別できるようにするため。
	$effect(() => {
		const name = channelInfo?.name?.trim();
		const title = name ? `${name} - PSTPlayer` : 'PSTPlayer';
		import('@tauri-apps/api/window')
			.then(({ getCurrentWindow }) => getCurrentWindow().setTitle(title))
			.catch(() => undefined);
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
			statusAtMs = Date.now();
		} catch (e) {
			console.warn('channel info fetch failed', e);
		}
		if (channelInfo?.url) {
			await tryLoadBoard(channelInfo.url);
		}
		await maybeAutoRecord();
	}

	async function maybeAutoRecord() {
		// 既に録画中なら何もしない (二重起動防止)。channelInfo が無い時
		// もスキップ。お気に入りルールにマッチかつ auto_record=true なら
		// 録画開始する。CLI 引数 --record-on-start でも強制起動 (favorites
		// と独立、hub の「視聴 + 録画」が使う)。
		if (!channelInfo) return;
		if (recordPath) return;
		try {
			const cli = await getCliArgs();
			// 1. --record-on-start で強制録画 (hub の「視聴+録画」spawn)
			if (cli.record_on_start) {
				const path = await playerRecordStart(channelInfo.name);
				recordPath = path;
				notify('録画開始 (CLI --record-on-start)', path);
				return;
			}
			// 2. 通常の auto_record ルール判定
			const cfg = await getConfig();
			const rules = cfg?.favorites?.rules as FavoriteRule[] | undefined;
			const fav = firstFavoriteMatch(rules, {
				name: channelInfo.name,
				genre: channelInfo.genre,
				desc: channelInfo.desc,
				comment: channelInfo.comment,
			});
			// action !== 'show' (= ignore / block) は自動録画しない
			if (fav?.auto_record && (!fav.action || fav.action === 'show')) {
				const path = await playerRecordStart(channelInfo.name);
				recordPath = path;
				notify(`自動録画開始 (${fav.name || 'お気に入り'})`, path);
			}
		} catch (e) {
			console.warn('auto-record failed', e);
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

	// コンタクト URL がスレッドを指す場合はそのスレッド key を、板 (掲示板)
	// URL の場合は null を返す。read.cgi / rawmode.cgi / dat 形式、または
	// したらば短縮スレ形式 (cat/board/key の 3 階層) をスレッドとみなす。
	// 板 URL (cat/board の 2 階層 / 2ch の host/board) は null。
	function threadKeyFromContact(url: string): string | null {
		const hasCgi = /\/(?:read|rawmode|write)\.cgi\//.test(url) || /\/dat\/\d+\.dat/.test(url);
		if (hasCgi) {
			const m = url.match(/.*\/(\d+)(?:\/[^/]*)?\/?$/);
			return m ? m[1] : null;
		}
		const sm = url.match(/^https?:\/\/jbbs\.shitaraba\.net\/[^/]+\/[^/]+\/(\d+)\/?$/);
		if (sm) return sm[1];
		return null; // 板 URL
	}

	async function tryLoadBoard(contactUrl: string) {
		try {
			// BBS 読み込み前に URL を正規化する。コンタクト/手入力 URL の末尾に
			// ブラウザ用の読み出し範囲 (/l50, /l30, /501-1000 等) が付いていても、
			// スレッドを指す場合は /{key}/ に畳んでから各 API に渡す。
			const key = threadKeyFromContact(contactUrl);
			const loadUrl = key ? normalizeThreadUrl(contactUrl, key) : contactUrl;
			threadList = await listThreads(loadUrl);
			// 板トップ URL を先に確定する (板 URL 時の最新スレ選択で使う)。
			try {
				currentBoardUrl = await boardUrlOf(loadUrl);
			} catch {
				currentBoardUrl = null;
			}
			fetchBoardSetting(loadUrl)
				.then((s) => (boardMaxRes = s.maxRes))
				.catch(() => (boardMaxRes = 0));

			if (key) {
				// コンタクトがスレッドを直接指している → そのスレを開く。
				// 既に満レスでも自動移動はしない (新着で上限到達時のみ移動する)。
				currentThreadUrl = loadUrl;
				await loadCurrentThread(true);
			} else {
				// コンタクトが板 URL → その板の最新スレを開く (#17)。
				await openNewestThread();
			}
		} catch (e) {
			console.warn('board load failed', e);
		}
	}

	/// 現チャンネルの板の最新スレ (作成時刻 = 数値 key が最大) を開く。
	/// コンタクトが板 URL のときの初期表示に使う。ポーリングからも再試行
	/// されるので、多重実行を `openingBoard` で防ぎ、各 invoke は withTimeout
	/// でハングを防いで finally でフラグを必ず解放する。
	let openingBoard = false;
	async function openNewestThread() {
		if (openingBoard) return;
		const board = currentBoardUrl;
		if (!board) return;
		openingBoard = true;
		try {
			let list = threadList;
			if (list.length === 0) {
				list = await withTimeout(listThreads(board), 10_000, 'listThreads');
				threadList = list;
			}
			if (list.length === 0) {
				currentThreadUrl = null;
				posts = [];
				return;
			}
			const newest = list.reduce((a, b) => (Number(b.key) > Number(a.key) ? b : a));
			if (!newest.key) return;
			currentThreadUrl = await withTimeout(threadUrlOf(board, newest.key), 8_000, 'threadUrlOf');
			fetchState = null;
			posts = [];
			await loadCurrentThread(true);
		} catch (e) {
			console.warn('openNewestThread failed', e);
		} finally {
			openingBoard = false;
		}
	}

	/// スレ一覧を取り直して作成時刻 (key) が最大の新スレへ自動移動する。
	/// 発火条件 (視聴中に新着レスで現スレが上限到達) は呼び出し側 (polling) が
	/// 判定する。移動前に 5 秒待機し、新スレが現スレと同じ (= まだ次スレが
	/// 立っていない) 場合は何もしない。
	async function advanceToNewestThread() {
		if (advancingThread) return;
		if (!currentThreadUrl || !currentBoardUrl) return;
		advancingThread = true;
		try {
			// 満レス検知から実移動まで 5 秒待つ。最後のレスを読む猶予に加え、
			// 次スレがまだ立っていない場合に立つのを待つ意味もある。
			await new Promise((r) => setTimeout(r, 5_000));
			const list = await listThreads(currentBoardUrl);
			if (list.length === 0) return;
			threadList = list;
			// 作成時刻 (= 数値 key) が最大のスレ = 最新スレ。
			const newest = list.reduce((a, b) => (Number(b.key) > Number(a.key) ? b : a));
			const curKey = currentThreadUrl.match(/(\d+)\/?$/)?.[1] ?? '';
			if (!newest.key || newest.key === curKey) return;
			const url = await threadUrlOf(currentBoardUrl, newest.key);
			currentThreadUrl = url;
			fetchState = null;
			posts = [];
			await loadCurrentThread(true);
		} catch (e) {
			console.warn('advance to newest thread failed', e);
		} finally {
			advancingThread = false;
		}
	}

	function isThreadGoneError(msg: string): boolean {
		// pst-core 側のエラー文言 (`dat returned 404 Not Found` /
		// `rawmode returned 404 Not Found` / `... 410 Gone`) に該当する
		// パターンを拾う。
		return /\b(404|410)\b/.test(msg) || /not found/i.test(msg) || /gone/i.test(msg);
	}

	// invoke の応答が起動直後の輻輳でまれに取りこぼされ、await が永久に
	// 解決しないことがある (Tauri の並行 invoke で観測)。そのまま放置すると
	// threadLoading が true で固着し、ポーリングの `!threadLoading` 条件で
	// 再試行が永久にスキップされてレスが 0 件のまま固まる。タイムアウトで
	// 強制 reject し、finally でフラグを解放して次回ポーリングに再試行を委ねる。
	function withTimeout<T>(p: Promise<T>, ms: number, label: string): Promise<T> {
		return new Promise<T>((resolve, reject) => {
			const t = setTimeout(() => reject(new Error(`${label} timed out after ${ms}ms`)), ms);
			p.then(
				(v) => {
					clearTimeout(t);
					resolve(v);
				},
				(e) => {
					clearTimeout(t);
					reject(e);
				},
			);
		});
	}

	async function loadCurrentThread(forceReset: boolean) {
		if (!currentThreadUrl) return;
		// 一度スレ落ち判定したら自動更新をスキップ (手動 reloadThreadFull
		// が呼ばれた場合のみ再試行: forceReset=true で死亡フラグを解除)。
		if (threadDead && !forceReset) return;
		if (forceReset) threadDead = false;
		threadLoading = true;
		if (forceReset) threadReloading = true;
		try {
			const prev = forceReset ? null : fetchState;
			const [newPosts, newState] = await withTimeout(
				fetchThread(currentThreadUrl, prev),
				12_000,
				'fetchThread',
			);
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
			if (forceReset) {
				// 配信表示 / スレ切替の初回は、最新レス (最下部) を表示した
				// 状態にする。多数レスでもレイアウト確定後に確実に最下部へ。
				await scrollPostsToBottomSettled();
			} else if (appendedNew && autoscroll && wasAtBottom) {
				// 新着レスは一瞬で飛ばず、設定速度でスムーズに流す (#20)。
				await tick();
				smoothScrollToBottom();
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
			const msg = errorMessage(e);
			lastError = msg;
			if (isThreadGoneError(msg)) {
				threadDead = true;
			}
		} finally {
			threadLoading = false;
			threadReloading = false;
		}
	}

	function startPolling() {
		if (threadTimer) clearInterval(threadTimer);
		if (playerTimer) clearInterval(playerTimer);
		// チャンネル状態 (status) は backend ポーラーが 5 秒間隔で
		// fetch → 'channel:status' event を emit する。フロントは
		// onMount で listen 済みなのでここでは start を呼ぶだけ。
		if (endpoint && channelId) {
			startChannelPolling(endpoint, channelId).catch(() => undefined);
		}
		threadTimer = setInterval(async () => {
			if (currentThreadUrl && !threadLoading) {
				// 起動時の輻輳等で 0 件のまま固着していたら、増分ではなく
				// 全件再取得 (forceReset) で回復を試みる。通常時は増分取得。
				const before = posts.length;
				await loadCurrentThread(posts.length === 0 && !threadDead);
				// 新着レスで現スレが上限到達したときだけ最新スレへ自動移動する。
				// (手動で満レスのスレを開いただけでは before==after で発火しない)
				const max = boardMaxRes > 0 ? boardMaxRes : THREAD_FULL_FALLBACK;
				if (before > 0 && posts.length > before && posts.length >= max) {
					await advanceToNewestThread();
				}
			} else if (!currentThreadUrl && currentBoardUrl && !threadLoading) {
				// 板 URL は判明しているのにスレ未選択 = 起動時に
				// openNewestThread が失敗した状態。再試行する。
				await openNewestThread();
			}
			refreshCountdown = REFRESH_SEC;
		}, REFRESH_SEC * 1_000);
		countdownTimer = setInterval(() => {
			if (refreshCountdown > 0) refreshCountdown -= 1;
			// uptime をリアルタイム (1 秒刻み) で進めるための時刻更新。
			nowMs = Date.now();
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
			// Bump は PeerCast にソース再取得を促すだけで、libmpv は切れた
			// ストリームを掴んだまま再生が止まる。再接続後のストリームを掴み
			// 直すため、明示的にプレイヤーを再ロードする (bump→reload 連動)。
			if (streamUrl) {
				await playerLoad(streamUrl);
			}
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	async function onStop() {
		if (!endpoint || !channelId) return;
		if (!confirm('チャンネルを切断します。よろしいですか?')) return;
		try {
			await stopChannel(endpoint, channelId);
			await stopChannelPolling().catch(() => undefined);
			await playerStop().catch((e) => console.warn('player_stop failed', e));
			streamUrl = null;
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	// ── BBS actions ──────────────────────────────────────────────────

	async function pickThread(entry: SubjectEntry) {
		await pickThreadFromPanel(entry);
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

	// 動画領域をクリックしたらウィンドウを前面化 + フォーカスする (#15)。
	// (子ウィンドウは WS_EX_TRANSPARENT でマウス透過なのでここに届く)
	async function onPlayerClick() {
		try {
			const { getCurrentWindow } = await import('@tauri-apps/api/window');
			const w = getCurrentWindow();
			// 既に前面 (フォーカス済み) なら何もしない。毎クリックで
			// show()/setFocus() を呼ぶと、ダブルクリック (全画面トグル) の
			// 1 回目で再アクティブ化が起き 2 回目が別クリック扱いになって
			// ダブルクリック検出を阻害するため、未フォーカス時のみ前面化する。
			if (await w.isFocused()) return;
			await w.show();
			await w.setFocus();
		} catch {
			/* best-effort */
		}
	}

	function onPlayerContextMenu(e: MouseEvent) {
		e.preventDefault();
		ctxMenu = { x: e.clientX, y: e.clientY };
		// メニュー展開のついでに最新履歴 / 録画状態を取得 (best-effort)。
		getHistory()
			.then((h) => (history = h))
			.catch(() => undefined);
		playerRecordPath()
			.then((p) => (recordPath = p))
			.catch(() => undefined);
	}

	async function ctxToggleRecord() {
		closeCtxMenu();
		try {
			if (recordPath) {
				await playerRecordStop();
				notify('録画停止', recordPath);
				recordPath = null;
			} else {
				const path = await playerRecordStart(channelInfo?.name);
				recordPath = path;
				notify('録画開始', path);
			}
		} catch (e) {
			lastError = errorMessage(e);
		}
	}

	async function openFromHistory(entry: HistoryEntry) {
		closeCtxMenu();
		pasteUrl = entry.url;
		await onPaste();
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

	// ── レス一覧 (.posts) の右クリックメニュー (C5) ───────────────────
	function onPostsContextMenu(e: MouseEvent) {
		e.preventDefault();
		postsMenu = { x: e.clientX, y: e.clientY };
	}
	function closePostsMenu() {
		postsMenu = null;
	}
	function scrollPostsTo(pos: 'top' | 'bottom') {
		closePostsMenu();
		const el = postsEl;
		if (!el) return;
		cancelSmoothScroll();
		el.scrollTo({ top: pos === 'top' ? 0 : el.scrollHeight, behavior: 'instant' });
	}
	function focusPostsFilter() {
		closePostsMenu();
		filterInput?.focus();
	}
	async function reloadThreadFromMenu() {
		closePostsMenu();
		if (currentThreadUrl) await loadCurrentThread(true);
	}
	function toggleAutoscrollFromMenu() {
		closePostsMenu();
		void toggleAutoscroll();
	}
	async function copyThreadUrl() {
		closePostsMenu();
		if (!currentThreadUrl) return;
		try {
			await navigator.clipboard.writeText(currentThreadUrl);
		} catch {
			/* permission denied */
		}
	}
	async function openThreadExternal() {
		closePostsMenu();
		if (currentThreadUrl) await openExternal(currentThreadUrl);
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

	// 連発抑止: 1 秒以内に複数回撮ったらトーストは「最後の 1 件」だけ
	// 出す (features.md §1.1 の挙動)。保存自体は毎回行う。
	let snapshotToastTimer: ReturnType<typeof setTimeout> | null = null;
	let snapshotLastPath = '';
	let snapshotBurstCount = 0;

	async function doSnapshot() {
		try {
			const path = await playerSnapshot(channelInfo?.name);
			snapshotLastPath = path;
			snapshotBurstCount += 1;
			if (snapshotToastTimer) clearTimeout(snapshotToastTimer);
			snapshotToastTimer = setTimeout(() => {
				const title =
					snapshotBurstCount > 1
						? `スナップショット保存 (${snapshotBurstCount} 枚)`
						: 'スナップショット保存';
				notify(title, snapshotLastPath);
				snapshotToastTimer = null;
				snapshotBurstCount = 0;
				snapshotLastPath = '';
			}, 1000);
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

	// スレ一覧はメインウィンドウ内のオーバーレイパネルで開閉する。
	// 別ウィンドウ (WebView2 セカンダリ) はコンポジタ凍結で描画されない
	// ため使わない (実機 QA で確定)。
	async function onOpenThreadList() {
		if (showThreadList) {
			showThreadList = false;
			return;
		}
		// 板が判明していなくてもパネルは開く。配信者がコンタクト URL を設定して
		// いないが実際は掲示板を使っているケースに備え、ユーザーが URL を手入力
		// してスレッドを適用できるようにするため。板がある時だけ一覧を取得する。
		showThreadList = true;
		if (currentBoardUrl || channelInfo?.url) await refreshThreadListPanel();
	}

	// スレ選択パネルでユーザーが手入力した URL (スレ or 板) を適用する。
	// コンタクト URL の有無に関係なく、任意の掲示板/スレッドを開ける。
	let manualThreadUrl = $state('');
	async function applyManualUrl() {
		const u = manualThreadUrl.trim();
		if (!u) return;
		showThreadList = false;
		// tryLoadBoard はスレ URL ならそのスレを、板 URL ならその板の最新スレを
		// 開く。手入力 URL もこれで賄える。
		await tryLoadBoard(u);
	}

	async function refreshThreadListPanel() {
		const board = currentBoardUrl ?? channelInfo?.url;
		if (!board) return;
		threadListLoading = true;
		try {
			threadList = await listThreads(board);
		} catch (e) {
			lastError = errorMessage(e);
		} finally {
			threadListLoading = false;
		}
	}

	async function pickThreadFromPanel(entry: SubjectEntry) {
		const board = currentBoardUrl ?? channelInfo?.url;
		if (!board) return;
		try {
			currentThreadUrl = await threadUrlOf(board, entry.key);
		} catch {
			currentThreadUrl = `${board.replace(/\/+$/, '')}/${entry.key}/`;
		}
		fetchState = null;
		posts = [];
		showThreadList = false;
		await loadCurrentThread(true);
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
		if (e instanceof CommandError) {
			// pst-core::AppError → IpcError code に対応する日本語メッセージ。
			// メッセージ自体は backend が組み立てているのでそのまま添える。
			switch (e.code) {
				case 'peercast_unreachable':
					return `PeerCast に接続できません。設定 → PeerCast のホスト / ポートを確認してください。 (${e.message})`;
				case 'board_regulated':
					return `書き込みが規制されています: ${e.message}`;
				case 'post_rejected':
					return `書き込みが拒否されました: ${e.message}`;
				case 'thread_gone':
					return `スレッドが見つかりません (削除 / 過去ログ送り)。 ${e.message}`;
				case 'invalid_url':
					return `URL が不正です: ${e.message}`;
				default:
					return `${e.code}: ${e.message}`;
			}
		}
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
	<div class="panes" style:grid-template-columns={showBbsPane ? `1fr 5px ${bbsWidth}px` : '1fr'}>
		<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
		<div
			class="player"
			onclick={onPlayerClick}
			oncontextmenu={onPlayerContextMenu}
			onwheel={onPlayerWheel}
			ondblclick={ctxToggleFullscreen}
			role="presentation"
		>
			{#if streamUrl}
				<!-- libmpv は専用の子ウィンドウをこの矩形に重ねて描く。
				     要素自体は空のままで OK (動画は native overlay)。 -->
				<div class="player-canvas" aria-label="再生中" bind:this={playerCanvasEl}></div>
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

		{#if showBbsPane}
			<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
			<div
				class="pane-splitter"
				role="separator"
				aria-label="BBS ペインの幅を変更"
				onmousedown={startBbsResize}
			></div>
		{/if}

		<div class="bbs">
			{#if showThreadList}
				<div class="threadlist-overlay">
					<div class="tl-head">
						<span class="tl-title">スレッド一覧</span>
						<button
							class="tl-btn"
							onclick={refreshThreadListPanel}
							disabled={threadListLoading}
							title="再取得">{threadListLoading ? '更新中…' : '↻'}</button
						>
						<button class="tl-btn" onclick={() => (showThreadList = false)} title="閉じる">✕</button
						>
					</div>
					<!-- 任意 URL の手入力。配信者がコンタクト URL 未設定でも、実際に
					     使っている掲示板/スレッドの URL を貼って適用できる。 -->
					<form
						class="tl-urlbar"
						onsubmit={(e) => {
							e.preventDefault();
							applyManualUrl();
						}}
					>
						<input
							type="text"
							bind:value={manualThreadUrl}
							placeholder="掲示板/スレッドの URL を貼り付けて適用 (コンタクト未設定でも可)"
							autocomplete="off"
							spellcheck="false"
						/>
						<button class="tl-btn" type="submit" disabled={!manualThreadUrl.trim()}>適用</button>
					</form>
					<ul class="tl-list">
						{#each threadList as t (t.key)}
							<li>
								<button class="tl-item" onclick={() => pickThreadFromPanel(t)}>
									<span class="tl-item-title">{t.title}</span>
									<span class="tl-item-count">({t.count})</span>
								</button>
							</li>
						{/each}
						{#if !threadListLoading && threadList.length === 0}
							<li class="tl-empty">スレッドがありません。</li>
						{/if}
					</ul>
				</div>
			{/if}
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
						placeholder="検索 / >>123 / id:xxx で抽出"
						title="検索:本文-名前-ID-番号 / >>N or >>N-M でレス番号抽出 / id:xxx で同一 ID 抽出"
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
					onwheel={cancelSmoothScroll}
					onclick={onPostsClick}
					oncontextmenu={onPostsContextMenu}
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
				{#if threadReloading}
					<!-- 絶対配置のオーバーレイにして .posts の高さに影響させない。
					     兄弟 flex アイテムにすると出入りでスクロールがビクンと
					     動く。さらに 5 秒毎の自動更新では出さず (threadReloading は
					     forceReset 時のみ)、点滅で鬱陶しくならないようにする。 -->
					<div class="refresh-indicator">更新中…</div>
				{/if}
			{/if}
		</div>
	</div>

	<!-- Thread title bar (オレンジ) -->
	<div class="thread-bar">
		<button
			class="thread-bar-button"
			title="スレッド一覧 / URL 手入力を開く"
			onclick={onOpenThreadList}
			disabled={!streamUrl}
		>
			<!-- 先頭にもスペーサーを入れ、末尾の t-grow と挟んでスレタイを
			     中央寄せにする (#16)。 -->
			<span class="t-grow"></span>
			{#if currentThreadUrl}
				<span class="t-title-main" title={currentThreadUrl}>
					{currentThreadTitle || '(無題)'}
				</span>
				<span class="t-count-main">({posts.length})</span>
				{#if threadDead}
					<span
						class="t-dead"
						title="スレッドが見つかりません。Ctrl+Shift+R で再取得を試行できます。"
					>
						💀 落ち
					</span>
				{/if}
			{:else}
				<span class="muted">— スレッド未選択 —</span>
			{/if}
			<span class="t-grow"></span>
			{#if currentThreadUrl && !threadDead}
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
				<button
					class="ctx-item"
					onclick={() => {
						closeCtxMenu();
						toggleAutoscroll();
					}}
				>
					{autoscroll ? '✓' : '　'} 新着レス自動スクロール
				</button>
				<button class="ctx-item" onclick={ctxOpenContactUrl} disabled={!channelInfo?.url}>
					🔗 コンタクト URL を開く
				</button>
				<button class="ctx-item" onclick={ctxCopyChannelUrl}>📋 チャンネル URL をコピー</button>
				<button
					class="ctx-item"
					onclick={() => {
						closeCtxMenu();
						showChannelDetails = true;
					}}
					disabled={!channelInfo}
				>
					📊 チャンネル詳細...
				</button>
				<div class="ctx-sep"></div>
				<div class="ctx-sub-host">
					<button class="ctx-item ctx-has-sub" type="button">
						📐 サイズ <span class="ctx-arrow">▶</span>
					</button>
					<div class="ctx-submenu">
						{#each SIZE_PERCENTS as pct, i (pct)}
							<button
								class="ctx-item"
								onclick={() => {
									closeCtxMenu();
									applySizePreset(i + 1);
								}}>{pct}%</button
							>
						{/each}
					</div>
				</div>
				<div class="ctx-sub-host">
					<button class="ctx-item ctx-has-sub" type="button">
						📺 アスペクト比 <span class="ctx-arrow">▶</span>
					</button>
					<div class="ctx-submenu">
						{#each ASPECT_PRESETS as ap, i (ap.label)}
							<button
								class="ctx-item"
								onclick={() => {
									closeCtxMenu();
									applyAspectPreset(i + 1);
								}}>{ap.label}</button
							>
						{/each}
					</div>
				</div>
				<div class="ctx-sub-host">
					<button class="ctx-item ctx-has-sub" type="button" disabled={history.length === 0}>
						🕒 視聴履歴 <span class="ctx-arrow">▶</span>
					</button>
					{#if history.length > 0}
						<div class="ctx-submenu ctx-submenu-wide">
							{#each history.slice(0, 12) as h (h.url)}
								<button class="ctx-item" onclick={() => openFromHistory(h)} title={h.url}>
									{h.channelName || h.url}
								</button>
							{/each}
						</div>
					{/if}
				</div>
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
				<button
					class="ctx-item"
					onclick={ctxToggleRecord}
					disabled={!streamUrl}
					title={recordPath ?? '視聴中の配信を再エンコードせず保存します'}
				>
					{recordPath ? '⏹ 録画停止' : '⏺ 録画開始'}
				</button>
				<button class="ctx-item" onclick={onOpenSettings}>⚙ 設定...</button>
				<button class="ctx-item" onclick={onOpenThreadList} disabled={!channelInfo?.url}>
					≡ スレ一覧を開く
				</button>
				<button
					class="ctx-item"
					onclick={() => {
						closeCtxMenu();
						openYpList();
					}}
				>
					📡 YP チャンネル一覧
				</button>
			</div>
		</div>
	{/if}

	<!-- レス一覧 (.posts) 右クリックメニュー (C5) -->
	{#if postsMenu}
		<!-- svelte-ignore a11y_no_noninteractive_element_interactions a11y_click_events_have_key_events -->
		<div
			class="ctx-backdrop"
			role="presentation"
			onclick={closePostsMenu}
			oncontextmenu={(e) => {
				e.preventDefault();
				closePostsMenu();
			}}
		>
			<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
			<div
				class="ctx"
				role="menu"
				tabindex="-1"
				style="left: {Math.min(postsMenu.x, window.innerWidth - 220)}px; top: {Math.min(
					postsMenu.y,
					window.innerHeight - 260,
				)}px"
				onclick={(e) => e.stopPropagation()}
			>
				<button class="ctx-item" onclick={focusPostsFilter}>🔍 レスを検索 / 絞り込み</button>
				<button class="ctx-item" onclick={() => scrollPostsTo('top')}>⤒ 最上部へ</button>
				<button class="ctx-item" onclick={() => scrollPostsTo('bottom')}>⤓ 最下部へ</button>
				<div class="ctx-sep"></div>
				<button class="ctx-item" onclick={toggleAutoscrollFromMenu}>
					{autoscroll ? '✓' : '　'} 新着レス自動スクロール
				</button>
				<button class="ctx-item" onclick={reloadThreadFromMenu} disabled={!currentThreadUrl}>
					↻ スレッド再取得
				</button>
				<div class="ctx-sep"></div>
				<button class="ctx-item" onclick={copyThreadUrl} disabled={!currentThreadUrl}>
					📋 スレッド URL をコピー
				</button>
				<button class="ctx-item" onclick={openThreadExternal} disabled={!currentThreadUrl}>
					🌐 スレッドをブラウザで開く
				</button>
			</div>
		</div>
	{/if}

	<!-- Channel details modal (PeerCast info + status の全フィールド) -->
	{#if showChannelDetails && channelInfo}
		<div
			class="popup-backdrop"
			role="presentation"
			onclick={() => (showChannelDetails = false)}
		></div>
		<div class="details-modal" role="dialog" aria-modal="true" aria-label="チャンネル詳細">
			<div class="popup-head">
				<span>📊 {channelInfo.name || '(unnamed)'}</span>
				<button class="popup-close" onclick={() => (showChannelDetails = false)}>×</button>
			</div>
			<div class="details-body">
				<h4>チャンネル情報</h4>
				<dl class="details-grid">
					<dt>名前</dt>
					<dd>{channelInfo.name || '-'}</dd>
					<dt>ジャンル</dt>
					<dd>{channelInfo.genre || '-'}</dd>
					<dt>詳細</dt>
					<dd>{channelInfo.desc || '-'}</dd>
					<dt>コメント</dt>
					<dd>{channelInfo.comment || '-'}</dd>
					<dt>コンタクト URL</dt>
					<dd>
						{#if channelInfo.url}
							{@const contactUrl = channelInfo.url}
							<a
								class="external-static"
								href={contactUrl}
								onclick={(e) => {
									e.preventDefault();
									openExternal(contactUrl);
								}}>{contactUrl}</a
							>
						{:else}-{/if}
					</dd>
					<dt>形式</dt>
					<dd>
						{channelInfo.contentType || '-'}
						<span class="muted"
							>/ {channelInfo.mimeType || '-'} / {channelInfo.streamType || '-'} / .{channelInfo.streamExt ||
								'-'}</span
						>
					</dd>
					<dt>ビットレート</dt>
					<dd>{channelInfo.bitrate || 0} kbps</dd>
				</dl>
				{#if channelStatus}
					<h4>接続状態</h4>
					<dl class="details-grid">
						<dt>ステータス</dt>
						<dd>{channelStatus.status || '-'}</dd>
						<dt>稼働時間</dt>
						<dd>{formatUptime(liveUptimeSec)}</dd>
						<dt>ローカル接続</dt>
						<dd>
							直 {channelStatus.localDirects} / リレー {channelStatus.localRelays}
						</dd>
						<dt>全体接続</dt>
						<dd>
							直 {channelStatus.totalDirects} / リレー {channelStatus.totalRelays}
						</dd>
						<dt>受信中</dt>
						<dd>{channelStatus.isReceiving ? 'はい' : 'いいえ'}</dd>
						<dt>配信元</dt>
						<dd>{channelStatus.isBroadcasting ? 'はい' : 'いいえ'}</dd>
						<dt>リレー枠</dt>
						<dd>{channelStatus.isRelayFull ? '満杯' : '空きあり'}</dd>
						<dt>直接枠</dt>
						<dd>{channelStatus.isDirectFull ? '満杯' : '空きあり'}</dd>
					</dl>
				{/if}
				{#if playerStat}
					<h4>再生</h4>
					<dl class="details-grid">
						<dt>解像度</dt>
						<dd>{playerStat.width ?? '-'} × {playerStat.height ?? '-'}</dd>
						<dt>FPS</dt>
						<dd>{playerStat.fps?.toFixed(2) ?? '-'}</dd>
					</dl>
				{/if}
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
			<span class="s-up">{formatUptime(liveUptimeSec)}</span>
			<span class="s-vol" title="マウスホイールで音量調整">♪ {volume}</span>
			<span class="s-actions">
				<button onclick={onBump} title="再接続 (Bump)">↻</button>
				<button onclick={onStop} title="切断 (Stop)">■</button>
				<button onclick={onOpenSettings} title="設定">⚙</button>
			</span>
		{:else}
			<span class="muted">未接続</span>
		{/if}
		{#if currentThreadUrl}
			<!-- スレッド表示中のレス件数。`statusLine` $derived は posts を
			     依存に持たないので分けて直接バインドする (これで Svelte 5 で
			     確実に reactive になる)。フィルタ中はそのカウントも併記。 -->
			<span class="s-posts" title="現スレッドのレス件数 (フィルタ中は表示中 / 全件)">
				📝 {#if visiblePosts.length !== posts.length}{visiblePosts.length} /
				{/if}{posts.length}
			</span>
		{/if}
		{#if reconnectStatus}
			<span class="reconnect" title="自動再接続の状態 (詳細は engine.rs)">⟳ {reconnectStatus}</span>
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
		/* 既定。実際の列幅はインライン style (bbsWidth) で上書きされる。 */
		grid-template-columns: 1fr 5px 320px;
		min-height: 0; /* allow children to shrink */
	}

	/* プレイヤーと BBS ペインの境界スプリッター (ドラッグで BBS 幅変更) (#14)。 */
	.pane-splitter {
		cursor: col-resize;
		background: var(--border);
		min-height: 0;
	}
	.pane-splitter:hover {
		background: var(--accent);
	}

	.player {
		/* libmpv は専用の子ウィンドウ (player/embed.rs) を .player-canvas の
		   矩形に重ねて描く。ここは未再生時 / 子ウィンドウ未配置時の地色。 */
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
		   まわりだけが黒。
		   フィルタ行を上端固定にしてレス一覧 (.posts) 側をスクロール
		   コンテナにするため flex column。以前は .bbs 自身がスクロール
		   していたが、JS の scrollPostsToBottom は .posts を対象にしており
		   「最下部へスクロール」が常に no-op になっていた (実機 QA で発覚)。 */
		background: var(--bg-elev);
		border-left: 1px solid var(--border);
		display: flex;
		flex-direction: column;
		overflow: hidden;
		min-height: 0;
		font-size: 0.85rem;
		color: var(--fg);
		position: relative;
	}

	/* スレ一覧オーバーレイ (別ウィンドウの代替)。BBS ペイン上に被せる。 */
	.threadlist-overlay {
		position: absolute;
		inset: 0;
		z-index: 20;
		background: var(--bg-elev);
		display: flex;
		flex-direction: column;
		min-height: 0;
	}
	.tl-head {
		display: flex;
		align-items: center;
		gap: 0.4rem;
		padding: 0.4rem 0.6rem;
		border-bottom: 1px solid var(--border);
		flex: 0 0 auto;
	}
	.tl-title {
		flex: 1;
		font-weight: 600;
	}
	.tl-btn {
		background: var(--bg);
		color: inherit;
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		padding: 0.15rem 0.5rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.8rem;
	}
	.tl-btn:disabled {
		opacity: 0.6;
		cursor: default;
	}
	.tl-urlbar {
		display: flex;
		gap: 0.4rem;
		padding: 0.4rem 0.6rem;
		border-bottom: 1px solid var(--border);
		flex: 0 0 auto;
	}
	.tl-urlbar input {
		flex: 1;
		min-width: 0;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.25rem 0.45rem;
		font-family: inherit;
		font-size: 0.8rem;
	}
	.tl-urlbar input:focus {
		outline: 2px solid var(--accent);
		border-color: transparent;
	}

	.tl-list {
		list-style: none;
		margin: 0;
		padding: 0;
		overflow-y: auto;
		flex: 1 1 0;
		min-height: 0;
	}
	.tl-item {
		display: flex;
		width: 100%;
		text-align: left;
		background: transparent;
		color: inherit;
		border: none;
		border-bottom: 1px solid var(--border);
		padding: 0.45rem 0.6rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.85rem;
	}
	.tl-item:hover {
		background: var(--border);
	}
	.tl-item-title {
		flex: 1;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.tl-item-count {
		color: var(--fg-muted);
		font-size: 0.78rem;
		margin-left: 0.5rem;
	}
	.tl-empty {
		padding: 0.6rem;
		color: var(--fg-muted);
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
		/* 実際のスクロールコンテナ。scrollPostsToBottom / isNearBottom が
		   この要素 (postsEl) を対象にしている。 */
		flex: 1 1 0;
		min-height: 0;
		overflow-y: auto;
	}

	/* 自動更新インジケータ。.bbs に対する絶対配置でレイアウトに影響させない
	   (= スクロール領域の高さを変えない)。 */
	.refresh-indicator {
		position: absolute;
		right: 0.5rem;
		bottom: 0.3rem;
		z-index: 6;
		pointer-events: none;
		font-size: 0.72rem;
		color: var(--fg-muted);
		background: color-mix(in srgb, var(--bg-elev) 80%, transparent);
		padding: 0.05rem 0.35rem;
		border-radius: 3px;
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
	.t-dead {
		color: #ffb3a2;
		font-size: 0.78rem;
		font-weight: 600;
		margin-left: 0.4rem;
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
	.s-vol,
	.s-posts {
		color: rgba(255, 255, 255, 0.85);
	}
	.s-posts {
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
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

	.hint {
		color: var(--fg-dim);
		margin: 0 0 0.4rem;
	}

	.err {
		color: var(--err);
		margin-left: auto;
	}

	.reconnect {
		color: #f5b942;
		margin-left: auto;
		font-weight: 500;
	}

	.reconnect + .err {
		margin-left: 0.5rem;
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

	.details-modal {
		position: fixed;
		top: 50%;
		left: 50%;
		transform: translate(-50%, -50%);
		z-index: 210;
		min-width: 400px;
		max-width: min(620px, calc(100vw - 32px));
		max-height: calc(100vh - 64px);
		background: var(--bg-elev);
		border: 1px solid var(--border-strong);
		border-radius: 8px;
		display: flex;
		flex-direction: column;
		box-shadow: 0 12px 36px rgba(0, 0, 0, 0.55);
	}
	.details-body {
		overflow-y: auto;
		padding: 0.6rem 1rem 1rem;
	}
	.details-body h4 {
		margin: 0.6rem 0 0.3rem;
		font-size: 0.78rem;
		color: var(--fg-muted);
		text-transform: uppercase;
		letter-spacing: 0.05em;
		border-bottom: 1px solid var(--border);
		padding-bottom: 0.2rem;
	}
	.details-body h4:first-child {
		margin-top: 0;
	}
	.details-grid {
		display: grid;
		grid-template-columns: 8rem 1fr;
		gap: 0.3rem 0.8rem;
		margin: 0;
		font-size: 0.85rem;
	}
	.details-grid dt {
		color: var(--fg-muted);
	}
	.details-grid dd {
		margin: 0;
		word-break: break-word;
	}
	.details-grid a.external-static {
		color: var(--accent-link);
		text-decoration: underline;
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

	/* サブメニュー: 親 .ctx-sub-host を hover した時に右に展開。 */
	.ctx-sub-host {
		position: relative;
	}
	.ctx-has-sub {
		display: flex;
		justify-content: space-between;
		align-items: center;
	}
	.ctx-arrow {
		font-size: 0.7rem;
		color: var(--fg-muted);
		margin-left: 0.6rem;
	}
	.ctx-submenu {
		display: none;
		position: absolute;
		top: 0;
		left: 100%;
		min-width: 140px;
		background: var(--bg-elev);
		border: 1px solid var(--border-strong);
		border-radius: 6px;
		padding: 0.25rem 0;
		box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
		z-index: 1;
	}
	.ctx-submenu-wide {
		min-width: 260px;
		max-width: 360px;
	}
	.ctx-submenu .ctx-item {
		white-space: nowrap;
		overflow: hidden;
		text-overflow: ellipsis;
	}
	/* hover + focus-within の両方で出すことでキーボードフォーカスでも開く。 */
	.ctx-sub-host:hover > .ctx-submenu,
	.ctx-sub-host:focus-within > .ctx-submenu {
		display: block;
	}

	.filter-bar {
		display: flex;
		gap: 0.3rem;
		padding: 0.3rem 0.5rem;
		border-bottom: 1px solid var(--border);
		background: var(--bg-elev);
		color: var(--fg);
		/* .bbs が flex column になり .posts がスクロールするので、
		   フィルタ行は flex item として上端に固定される (sticky 不要)。 */
		flex: 0 0 auto;
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
