<script lang="ts">
	// ブラウザ視聴ページ (ハイブリッド視聴の browser 側 / player = 視聴+レス+書き込み)。
	// pst-server が配信する Web UI のハブから別タブで開く。動画は HLS
	// (/hls/<id>/index.m3u8 を hls.js)、BBS は REST (/api/board,/api/thread,
	// /api/thread/post) で読み書きする。レス本文は format.ts の renderBodyHtml
	// (クライアント側で全エスケープ + アンカー/URL リンク化) で安全に描画する
	// ので、Rust 側のサニタイズ (HTML 表示モード専用) は使わない。
	import { onDestroy, onMount, tick } from 'svelte';
	import {
		boardUrlOf,
		fetchBoardSetting,
		fetchChannelInfo,
		fetchThread,
		getConfig,
		postToThread,
		type ChannelInfo,
		type FetchState,
		type Post,
	} from '$lib/api';
	import { normalizeThreadUrl, threadKeyFromContact } from '$lib/bbs-url';
	import { selectLiveThread, THREAD_FULL_FALLBACK } from '$lib/thread-select';
	import { renderBodyHtml, renderIdHtml } from '$lib/format';

	let video = $state<HTMLVideoElement | undefined>();
	let channelId = $state<string | null>(null);
	let tip: string | null = null;
	let videoStatus = $state('読み込み中…');
	let hls: {
		destroy(): void;
		startLoad(): void;
		recoverMediaError(): void;
		swapAudioCodec(): void;
	} | null = null;
	// mpegts.js (FLV 直結) プレイヤー。MSE が使える環境ではこちらを優先。
	let flvPlayer: { destroy(): void } | null = null;
	// 再生停滞ウォッチドッグ (再生中なのに currentTime が進まない状態を検知)。
	let stallTimer: ReturnType<typeof setInterval> | null = null;
	let lastTime = -1;
	let stallTicks = 0;
	// hls.js 作り直し回数 (無限リカバリループ防止)。再生が進んだらリセット。
	let videoRestarts = 0;

	let channelInfo = $state<ChannelInfo | null>(null);
	let bbsUrl = $state('');
	let posts = $state<Post[]>([]);
	let bbsState: FetchState | null = null;
	let bbsError = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;
	// 満レス移動用: 板トップ URL と最大レス数のキャッシュ + 多重実行ガード。
	let playerBoardUrl: string | null = null;
	let playerMaxRes = 0;
	let advancingBbs = false;

	// 書き込みフォーム。名前 / メール欄は UI に出さず、設定の既定値
	// (既定: 名前 = 空, メール = sage) をそのまま使う。
	let postName = '';
	let postMail = 'sage';
	let postBody = $state('');
	let posting = $state(false);
	// レス一覧のスクロール制御 (最下部追従)。
	let postsEl = $state<HTMLDivElement | undefined>();

	onMount(() => {
		const params = new URLSearchParams(window.location.search);
		channelId = params.get('id');
		tip = params.get('tip');
		if (!channelId) {
			videoStatus = 'エラー: ?id=<channel_id> が指定されていません';
			return;
		}
		// 書き込み既定値 (pst-server の [bbs]) を読む。失敗時は既定のまま。
		getConfig()
			.then((cfg) => {
				postName = cfg?.bbs?.defaultName ?? '';
				postMail = cfg?.bbs?.defaultMail ?? 'sage';
			})
			.catch(() => undefined);
		void startVideo(channelId);
		void startBbs(channelId);
	});

	onDestroy(() => {
		flvPlayer?.destroy();
		hls?.destroy();
		if (pollTimer) clearInterval(pollTimer);
		if (stallTimer) clearInterval(stallTimer);
	});

	/// FLV 直結再生 (mpegts.js)。成功したら true。
	async function startFlv(id: string, el: HTMLVideoElement): Promise<boolean> {
		try {
			const mpegts = (await import('mpegts.js')).default;
			if (!mpegts.isSupported()) return false;
			const feat = mpegts.getFeatureList();
			if (!feat.mseLivePlayback) return false;
			const url =
				`/stream/${encodeURIComponent(id)}.flv` + (tip ? `?tip=${encodeURIComponent(tip)}` : '');
			videoStatus = '接続中…';
			const player = mpegts.createPlayer(
				{ type: 'flv', isLive: true, url },
				{
					// ライブ端追跡: 遅延が 3 秒を超えたら追いかける。
					enableStashBuffer: false,
					liveBufferLatencyChasing: true,
					liveBufferLatencyMaxLatency: 3.0,
					liveBufferLatencyMinRemain: 0.5,
					autoCleanupSourceBuffer: true,
				},
			);
			player.on(mpegts.Events.ERROR, () => {
				// ネットワーク切断・デコード不能等。作り直しで再接続する
				// (上限は videoRestarts で共通管理)。
				scheduleVideoRebuild(id, 2000);
			});
			player.attachMediaElement(el);
			player.load();
			el.addEventListener(
				'canplay',
				() => {
					if (!el.paused) return;
					el.play().catch(() => {
						el.muted = true;
						el.play().catch(() => undefined);
					});
				},
				{ once: true },
			);
			flvPlayer = player;
			videoStatus = '';
			startStallWatchdog(el);
			return true;
		} catch {
			return false;
		}
	}

	/// 現行プレイヤーを破棄して startVideo をやり直す (再接続)。
	function scheduleVideoRebuild(id: string, delayMs: number) {
		try {
			flvPlayer?.destroy();
		} catch {
			/* ignore */
		}
		flvPlayer = null;
		try {
			hls?.destroy();
		} catch {
			/* ignore */
		}
		hls = null;
		videoRestarts += 1;
		if (videoRestarts > 5) {
			videoStatus = '再生できません (ストリームを再生できませんでした)';
			return;
		}
		setTimeout(() => void startVideo(id), delayMs);
	}

	async function startVideo(id: string) {
		const el = video;
		if (!el) return;
		// リアルタイム性を重視し、MSE が使える環境では FLV 直結
		// (mpegts.js) を最優先する。HLS は約8秒セグメント×同期3本ぶん
		// (~25秒) 構造的に遅延するため、フォールバック専用
		// (iPhone Safari 等の MSE 無し環境)。
		if (await startFlv(id, el)) return;
		// playlist は /hls/{id} (ベアパス)。実機 PeerCastStation は
		// /hls/{id}/index.m3u8 を 404/503 にする。未リレー join 用に
		// tip をクエリで引き継ぐ (プロキシが上流へ透過する)。
		const src = `/hls/${encodeURIComponent(id)}` + (tip ? `?tip=${encodeURIComponent(tip)}` : '');
		// MSE がある環境では hls.js を最優先する。canPlayType は Windows の
		// Chrome でも 'maybe' を返すことがあり (実機 QA)、それを信じて
		// ネイティブ再生に振ると無反応のまま止まる。ネイティブ HLS は
		// hls.js が使えない環境 (iOS Safari 等) のフォールバック。
		try {
			const Hls = (await import('hls.js')).default;
			if (Hls.isSupported()) {
				// join 前にセッション URL を解決し、プレイリストが育つ
				// (セグメント3本 ≒ 25秒) まで待つ。リロード直後は上流の
				// セグメンターがリセットされ、セグメント1本のプレイリスト
				// を掴まされて 8 秒ごとに枯渇→詰まりを繰り返す (実機 QA:
				// リロード後 seq=75→タイムアウト→seq=2/n=1 を観測)。
				videoStatus = '接続中…';
				let playUrl = src;
				try {
					const r0 = await fetch(src);
					if (r0.ok) {
						playUrl = r0.url || src;
						let text = await r0.text();
						for (let i = 0; i < 10; i++) {
							const n = (text.match(/#EXTINF/g) || []).length;
							if (n >= 3) break;
							await new Promise((res) => setTimeout(res, 2000));
							const r = await fetch(playUrl);
							if (!r.ok) break;
							text = await r.text();
						}
					}
				} catch {
					/* 事前確認に失敗しても従来どおり src で再生を試みる */
				}
				// PeerCastStation の HLS は約8秒セグメント×5本 (窓 ~42秒)。
				// 同期位置は 3 本 (≒25秒遅延)。2 本だとクッションが薄く、
				// 作り直し直後にセグメント到着間隔 (~8秒) ごとに詰まる。
				// 3 本でも窓の後端 (ローテーションアウト) までは ~17秒の
				// マージンがある。liveMaxLatencyDurationCount による自動
				// シークは、途中参加時に整合が壊れた状態で 0 秒付近へ
				// 飛ばして「同じシーンのループ」を誘発するため使わない
				// (実機 QA)。
				const h = new Hls({
					enableWorker: true,
					lowLatencyMode: true,
					liveSyncDurationCount: 3,
				});
				// チャンネル join 直後は playlist の応答に時間がかかったり
				// エラーになることがある。fatal エラーで hls.js がロードを
				// 止めたままにならないよう、自動リカバリする (実機 QA:
				// リロード後にバッファ分だけ再生して止まる)。
				// MEDIA_ERROR の recoverMediaError() は失敗すると即また fatal が
				// 飛んでくるため、無制限に呼ぶと毎秒数回の detach/attach ループに
				// なる (実機 QA: デコード不能ストリームで画面が点滅し続けた)。
				// hls.js 推奨の 1回目 recover → 2回目 swapAudioCodec+recover →
				// それ以降は作り直し (回数上限つき) に制限する。
				let mediaRecovers = 0;
				let lastRecoverAt = 0;
				const rebuild = () => {
					h.destroy();
					if (hls !== h) return;
					hls = null;
					videoRestarts += 1;
					if (videoRestarts > 5) {
						videoStatus = '再生できません (ストリームをデコードできませんでした)';
						return;
					}
					setTimeout(() => void startVideo(id), 3000 * videoRestarts);
				};
				h.on(Hls.Events.ERROR, (_ev: unknown, data: { fatal: boolean; type: string }) => {
					if (!data.fatal) return;
					if (data.type === Hls.ErrorTypes.NETWORK_ERROR) {
						setTimeout(() => hls && h.startLoad(), 2000);
						return;
					}
					if (data.type === Hls.ErrorTypes.MEDIA_ERROR) {
						const now = Date.now();
						// しばらく安定していたら失敗カウントをリセット。
						if (now - lastRecoverAt > 15000) mediaRecovers = 0;
						lastRecoverAt = now;
						mediaRecovers += 1;
						if (mediaRecovers === 1) {
							h.recoverMediaError();
							return;
						}
						if (mediaRecovers === 2) {
							h.swapAudioCodec();
							h.recoverMediaError();
							return;
						}
					}
					// その他の fatal / 復旧しない MEDIA_ERROR は作り直す。
					rebuild();
				});
				// 再生開始はデータが揃ってから (canplay)。データの無い位置で
				// 先に play すると waiting→シーク→waiting と何度も詰まって
				// 見える (実機 QA: 作り直し直後)。また、リロード直後などは
				// 音声付き autoplay がブロックされるので、その場合は
				// ミュートで再生を開始する (音はコントロールで戻せる)。
				el.addEventListener(
					'canplay',
					() => {
						if (!el.paused) return;
						el.play().catch(() => {
							el.muted = true;
							el.play().catch(() => undefined);
						});
					},
					{ once: true },
				);
				h.loadSource(playUrl);
				h.attachMedia(el);
				hls = h;
				videoStatus = '';
				startStallWatchdog(el);
				return;
			}
		} catch {
			/* hls.js が読めない場合はネイティブへフォールバック */
		}
		if (el.canPlayType('application/vnd.apple.mpegurl')) {
			el.src = src;
			videoStatus = '';
		} else {
			videoStatus = 'このブラウザは HLS 再生に対応していません';
		}
	}

	async function startBbs(id: string) {
		// チャンネル join 直後は上流 PeerCast がまだ情報を持っておらず
		// 取得に失敗する (実機 QA)。しばらくリトライしてから諦める。
		for (let i = 0; i < 10 && !bbsUrl; i++) {
			try {
				// endpoint は browser では REST 側が無視する (path の channel id を使う)。
				channelInfo = await fetchChannelInfo({ host: '', port: 0 }, id);
				bbsUrl = channelInfo?.url ?? '';
			} catch {
				/* join 待ち。リトライする */
			}
			if (!bbsUrl) await new Promise((r) => setTimeout(r, 3000));
		}
		if (!bbsUrl) {
			bbsError = 'この配信にはコンタクト URL (BBS) がありません';
			return;
		}
		// コンタクト URL を正規化する。スレ URL は末尾範囲 (/l50 等) を
		// 畳む (そのまま API に渡すと classify が拒否する: 実機 QA)。
		// 板 URL は「並び上位で満レスでないスレ」を選出する。
		const key = threadKeyFromContact(bbsUrl);
		if (key) {
			bbsUrl = normalizeThreadUrl(bbsUrl, key);
		} else {
			try {
				playerBoardUrl = bbsUrl;
				const s = await fetchBoardSetting(bbsUrl).catch(() => null);
				playerMaxRes = s && s.maxRes > 0 ? s.maxRes : THREAD_FULL_FALLBACK;
				const live = await selectLiveThread(bbsUrl, playerMaxRes);
				if (live) bbsUrl = live.url;
			} catch {
				/* 選出できなければ板 URL のまま (reloadBbs がエラー表示する) */
			}
		}
		await reloadBbs();
		// コンタクトのスレが既に満レスなら別スレへ (初期表示リダイレクト)。
		await advanceIfFull();
		pollTimer = setInterval(() => {
			void (async () => {
				await reloadBbs();
				// 自動更新で満レスになったら、新スレが見つかるまで毎周期
				// 探索する (ブラウザ視聴に手動選択は無いので常に自動扱い)。
				await advanceIfFull();
			})();
		}, 10000);
	}

	/// 現スレが満レス (実レス数 >= 板の最大レス数) なら、板の並び上位から
	/// 満レスでないスレを選んで移動する。見つからなければ何もしない
	/// (ポーリングが次周期に再試行する)。
	async function advanceIfFull() {
		if (advancingBbs || !bbsUrl) return;
		advancingBbs = true;
		try {
			if (!playerMaxRes) {
				const s = await fetchBoardSetting(bbsUrl).catch(() => null);
				playerMaxRes = s && s.maxRes > 0 ? s.maxRes : THREAD_FULL_FALLBACK;
			}
			if (posts.length < playerMaxRes) return;
			if (!playerBoardUrl) playerBoardUrl = await boardUrlOf(bbsUrl);
			const curKey = threadKeyFromContact(bbsUrl);
			const live = await selectLiveThread(playerBoardUrl, playerMaxRes, curKey ? [curKey] : []);
			if (!live || live.key === curKey) return;
			bbsUrl = live.url;
			bbsState = null;
			posts = [];
			await reloadBbs();
		} catch {
			/* 判定に失敗しても現スレ表示を維持 */
		} finally {
			advancingBbs = false;
		}
	}

	/// 再生の停滞・巻き戻りを監視し、検知したらプレイヤーを作り直す。
	///
	/// 進行中セッションへの途中参加 (リロード) では hls.js のタイムライン
	/// 整合が壊れることがあり、セグメントを取得しても正位置に append
	/// できず、バッファ切れ→hls.js が後方バッファへシーク→同じ場面を
	/// ループする (実機 QA で確認)。この状態は startLoad 等の小手先では
	/// 復旧しないため、新規セッションで入り直すのが確実。
	function startStallWatchdog(el: HTMLVideoElement) {
		if (stallTimer) clearInterval(stallTimer);
		lastTime = -1;
		stallTicks = 0;
		let backJumps: number[] = [];
		const startedAt = Date.now();
		const rebuild = () => {
			const id = channelId;
			if (!id || (!hls && !flvPlayer)) return;
			scheduleVideoRebuild(id, 0);
		};
		stallTimer = setInterval(() => {
			if ((!hls && !flvPlayer) || el.paused) {
				stallTicks = 0;
				lastTime = el.currentTime;
				return;
			}
			// join 直後でまだ何も append されていない間は「停滞」ではない
			// (playlist の long-poll で最大 40 秒かかる)。ここで作り直すと
			// セッションを永遠に作り直し続けてしまう。
			if (el.readyState < 2 && el.buffered.length === 0) {
				stallTicks = 0;
				lastTime = el.currentTime;
				return;
			}
			const t = el.currentTime;
			if (t === lastTime) {
				stallTicks += 1;
				if (stallTicks >= 4) {
					// 8 秒進んでいない → 作り直し (新規セッションで入り直す)。
					stallTicks = 0;
					rebuild();
					return;
				}
			} else {
				stallTicks = 0;
				// hls.js の stall 復旧による後方シーク (ユーザー操作でない
				// 3 秒超の巻き戻り) がループの兆候。初回 join の整合レースは
				// 起動直後に出るため、起動 90 秒以内は 1 回で即作り直す。
				// 以降はユーザーの巻き戻し操作と区別するため 60 秒に 2 回で
				// 作り直す。
				if (t < lastTime - 3 && lastTime > 0) {
					const now = Date.now();
					backJumps = backJumps.filter((x) => now - x < 60000);
					backJumps.push(now);
					if (backJumps.length >= 2 || now - startedAt < 90000) {
						backJumps = [];
						rebuild();
						return;
					}
				} else if (t > lastTime) {
					videoRestarts = 0;
				}
			}
			lastTime = t;
		}, 2000);
	}

	/// レス一覧の末尾付近を見ているか (= 新着で追従してよいか)。
	function nearBottom(): boolean {
		if (!postsEl) return true;
		return postsEl.scrollHeight - postsEl.scrollTop - postsEl.clientHeight < 80;
	}

	async function reloadBbs() {
		try {
			// 初回、または末尾付近を見ているときだけ追従スクロールする
			// (過去レスを遡って読んでいる最中は動かさない)。
			const stick = posts.length === 0 || nearBottom();
			const [newPosts, state] = await fetchThread(bbsUrl, bbsState);
			bbsState = state;
			let changed = false;
			if (state.fullReload) {
				// スレ全体のスナップショット (dat 再構築等)。マージすると
				// 消えたレスが残るため置換する。
				posts = newPosts;
				changed = true;
			} else if (newPosts.length) {
				// 差分取得分を number でマージ (既存 + 新規)。
				const map = new Map(posts.map((p) => [p.number, p]));
				for (const p of newPosts) map.set(p.number, p);
				posts = [...map.values()].sort((a, b) => a.number - b.number);
				changed = true;
			}
			bbsError = null;
			if (changed && stick) {
				// デスクトップと同じく最下部 (最新レス) を表示した状態にする。
				await tick();
				if (postsEl) postsEl.scrollTop = postsEl.scrollHeight;
			}
		} catch (e) {
			bbsError = 'BBS の取得に失敗しました: ' + (e instanceof Error ? e.message : String(e));
		}
	}

	async function submitPost() {
		if (!postBody.trim() || posting || !bbsUrl) return;
		posting = true;
		try {
			await postToThread(bbsUrl, { name: postName, mail: postMail, body: postBody });
			postBody = '';
			await reloadBbs();
		} catch (e) {
			bbsError = '書き込みに失敗しました: ' + (e instanceof Error ? e.message : String(e));
		} finally {
			posting = false;
		}
	}

	function onBodyKeydown(e: KeyboardEvent) {
		// Ctrl/Cmd+Enter で送信。
		if ((e.ctrlKey || e.metaKey) && e.key === 'Enter') {
			e.preventDefault();
			void submitPost();
		}
	}
</script>

<svelte:head>
	<title>PSTPlayer · {channelInfo?.name || '視聴'}</title>
</svelte:head>

<main>
	<section class="video-pane">
		<!-- svelte-ignore a11y_media_has_caption -->
		<video bind:this={video} controls autoplay playsinline></video>
		{#if videoStatus}
			<p class="overlay">{videoStatus}</p>
		{/if}
	</section>

	<section class="bbs-pane">
		<header class="bbs-head">
			<span class="ch-name">{channelInfo?.name || channelId || ''}</span>
			<span class="ch-count">({posts.length})</span>
		</header>

		{#if bbsError}
			<div class="bbs-error">⚠ {bbsError}</div>
		{/if}

		<div class="posts" bind:this={postsEl}>
			{#each posts as p (p.number)}
				<article class="post">
					<div class="meta">
						<span class="num">{p.number}</span>
						<span class="name">{p.name}</span>
						{#if p.mail}<span class="mail">[{p.mail}]</span>{/if}
						<span class="date">{p.date}</span>
						{#if p.id}<span class="id">{@html renderIdHtml(p.id)}</span>{/if}
					</div>
					<!-- renderBodyHtml はクライアント側で全エスケープ済み (安全) -->
					<div class="body">{@html renderBodyHtml(p.body)}</div>
				</article>
			{/each}
			{#if posts.length === 0 && !bbsError}
				<p class="empty">レスがありません</p>
			{/if}
		</div>

		{#if bbsUrl}
			<form
				class="post-form"
				onsubmit={(e) => {
					e.preventDefault();
					void submitPost();
				}}
			>
				<textarea
					bind:value={postBody}
					onkeydown={onBodyKeydown}
					placeholder="本文 (Ctrl/Cmd+Enter で送信)"
					aria-label="本文"
					rows="2"
				></textarea>
				<button type="submit" disabled={posting || !postBody.trim()}>
					{posting ? '送信中…' : '書き込む'}
				</button>
			</form>
		{/if}
	</section>
</main>

<style>
	:global(html, body) {
		margin: 0;
		height: 100%;
		background: #000;
	}
	main {
		display: flex;
		height: 100vh;
		overflow: hidden;
	}
	.video-pane {
		position: relative;
		flex: 1 1 60%;
		min-width: 0;
		background: #000;
		display: flex;
		align-items: center;
		justify-content: center;
	}
	.video-pane video {
		width: 100%;
		height: 100%;
		max-height: 100vh;
		background: #000;
	}
	.overlay {
		position: absolute;
		top: 1rem;
		left: 1rem;
		color: #ddd;
		font-family: sans-serif;
		font-size: 14px;
		background: rgba(0, 0, 0, 0.6);
		padding: 0.4rem 0.8rem;
		border-radius: 4px;
	}
	.bbs-pane {
		flex: 0 0 380px;
		display: flex;
		flex-direction: column;
		background: #fff;
		color: #1a1a1a;
		font-family:
			'Yu Gothic UI',
			Meiryo,
			-apple-system,
			sans-serif;
		font-size: 13px;
		border-left: 1px solid #ccc;
	}
	.bbs-head {
		display: flex;
		gap: 0.4rem;
		align-items: baseline;
		padding: 0.4rem 0.6rem;
		background: #f4f4f4;
		border-bottom: 1px solid #d0d0d0;
	}
	.ch-name {
		font-weight: 600;
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}
	.ch-count {
		color: #888;
	}
	.bbs-error {
		padding: 0.4rem 0.6rem;
		background: #fff0f0;
		color: #c0392b;
		font-size: 12px;
	}
	.posts {
		flex: 1;
		overflow-y: auto;
		padding: 0.3rem 0.5rem;
	}
	.post {
		padding: 0.3rem 0;
		border-bottom: 1px solid #eee;
	}
	.post .meta {
		font-size: 11px;
		color: #555;
		display: flex;
		flex-wrap: wrap;
		gap: 0.3rem;
	}
	.post .num {
		color: #0a4cad;
		font-weight: 600;
	}
	.post .name {
		color: #1a7a1a;
		font-weight: 600;
	}
	.post .body {
		margin-top: 0.15rem;
		white-space: normal;
		word-break: break-word;
		line-height: 1.5;
	}
	.empty {
		color: #888;
		text-align: center;
		margin-top: 1rem;
	}
	.post-form {
		border-top: 1px solid #d0d0d0;
		padding: 0.4rem 0.5rem;
		background: #f8f8f8;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.post-form textarea {
		font: inherit;
		padding: 0.25rem 0.4rem;
		border: 1px solid #b0b0b0;
		border-radius: 3px;
		width: 100%;
		box-sizing: border-box;
	}
	.post-form button {
		align-self: flex-end;
		padding: 0.25rem 1rem;
		font: inherit;
		cursor: pointer;
		border: 1px solid #4a78c0;
		background: #5b8fd6;
		color: #fff;
		border-radius: 3px;
	}
	.post-form button:disabled {
		opacity: 0.5;
		cursor: not-allowed;
	}
	/* 狭い画面 (モバイル) は縦積み。 */
	@media (max-width: 700px) {
		main {
			flex-direction: column;
		}
		.video-pane {
			flex: 0 0 40vh;
		}
		.bbs-pane {
			flex: 1;
			border-left: none;
			border-top: 1px solid #ccc;
		}
	}
	:global(.body a.anchor) {
		color: #0a4cad;
		cursor: pointer;
	}
	:global(.body a.external) {
		color: #0a4cad;
	}
	:global(.id a.id-link) {
		color: #888;
		text-decoration: none;
	}
</style>
