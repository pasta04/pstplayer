<script lang="ts">
	// ブラウザ視聴ページ (ハイブリッド視聴の browser 側 / player = 視聴+レス+書き込み)。
	// pst-server が配信する Web UI のハブから別タブで開く。動画は HLS
	// (/hls/<id>/index.m3u8 を hls.js)、BBS は REST (/api/board,/api/thread,
	// /api/thread/post) で読み書きする。レス本文は format.ts の renderBodyHtml
	// (クライアント側で全エスケープ + アンカー/URL リンク化) で安全に描画する
	// ので、Rust 側のサニタイズ (HTML 表示モード専用) は使わない。
	import { onDestroy, onMount } from 'svelte';
	import {
		fetchChannelInfo,
		fetchThread,
		postToThread,
		type ChannelInfo,
		type FetchState,
		type Post,
	} from '$lib/api';
	import { renderBodyHtml, renderIdHtml } from '$lib/format';

	let video = $state<HTMLVideoElement | undefined>();
	let channelId = $state<string | null>(null);
	let tip: string | null = null;
	let videoStatus = $state('読み込み中…');
	let hls: { destroy(): void } | null = null;

	let channelInfo = $state<ChannelInfo | null>(null);
	let bbsUrl = $state('');
	let posts = $state<Post[]>([]);
	let bbsState: FetchState | null = null;
	let bbsError = $state<string | null>(null);
	let pollTimer: ReturnType<typeof setInterval> | null = null;

	// 書き込みフォーム。
	let postName = $state('');
	let postMail = $state('');
	let postBody = $state('');
	let posting = $state(false);

	onMount(() => {
		const params = new URLSearchParams(window.location.search);
		channelId = params.get('id');
		tip = params.get('tip');
		if (!channelId) {
			videoStatus = 'エラー: ?id=<channel_id> が指定されていません';
			return;
		}
		void startVideo(channelId);
		void startBbs(channelId);
	});

	onDestroy(() => {
		hls?.destroy();
		if (pollTimer) clearInterval(pollTimer);
	});

	async function startVideo(id: string) {
		const el = video;
		if (!el) return;
		// playlist は /hls/{id} (ベアパス)。実機 PeerCastStation は
		// /hls/{id}/index.m3u8 を 404/503 にする。未リレー join 用に
		// tip をクエリで引き継ぐ (プロキシが上流へ透過する)。
		const src = `/hls/${encodeURIComponent(id)}` + (tip ? `?tip=${encodeURIComponent(tip)}` : '');
		// Safari / iOS はネイティブ HLS 再生。
		if (el.canPlayType('application/vnd.apple.mpegurl')) {
			el.src = src;
			videoStatus = '';
			return;
		}
		try {
			const Hls = (await import('hls.js')).default;
			if (Hls.isSupported()) {
				const h = new Hls({ enableWorker: true, lowLatencyMode: true });
				h.loadSource(src);
				h.attachMedia(el);
				hls = h;
				videoStatus = '';
			} else {
				videoStatus = 'このブラウザは HLS 再生に対応していません';
			}
		} catch (e) {
			videoStatus = 'HLS の読み込みに失敗しました: ' + (e instanceof Error ? e.message : String(e));
		}
	}

	async function startBbs(id: string) {
		try {
			// endpoint は browser では REST 側が無視する (path の channel id を使う)。
			channelInfo = await fetchChannelInfo({ host: '', port: 0 }, id);
			bbsUrl = channelInfo?.url ?? '';
		} catch {
			/* channel info が取れなくても視聴は継続 */
		}
		if (!bbsUrl) {
			bbsError = 'この配信にはコンタクト URL (BBS) がありません';
			return;
		}
		await reloadBbs();
		pollTimer = setInterval(() => {
			void reloadBbs();
		}, 10000);
	}

	async function reloadBbs() {
		try {
			const [newPosts, state] = await fetchThread(bbsUrl, bbsState);
			bbsState = state;
			if (state.fullReload) {
				// スレ全体のスナップショット (dat 再構築等)。マージすると
				// 消えたレスが残るため置換する。
				posts = newPosts;
			} else if (newPosts.length) {
				// 差分取得分を number でマージ (既存 + 新規)。
				const map = new Map(posts.map((p) => [p.number, p]));
				for (const p of newPosts) map.set(p.number, p);
				posts = [...map.values()].sort((a, b) => a.number - b.number);
			}
			bbsError = null;
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

		<div class="posts">
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
				<div class="row">
					<input bind:value={postName} placeholder="名前" aria-label="名前" />
					<input bind:value={postMail} placeholder="メール (sage 等)" aria-label="メール" />
				</div>
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
	.post-form .row {
		display: flex;
		gap: 0.3rem;
	}
	.post-form input,
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
