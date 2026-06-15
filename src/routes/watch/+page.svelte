<script lang="ts">
	// ブラウザ HLS 視聴ページ (ハイブリッド視聴の browser 側)。
	// pst-server が配信する Web UI のハブから、チャンネルを「別タブ」で開いて
	// 視聴する。ネイティブアプリは libmpv、ブラウザはこの HLS プレイヤー。
	// pst-server の /hls/<id>/index.m3u8 (上流 PeerCastStation の HLS を透過)
	// を hls.js で再生する。Safari / iOS はネイティブ HLS 再生にフォールバック。
	import { onDestroy, onMount } from 'svelte';

	let video = $state<HTMLVideoElement | undefined>();
	let channelId = $state<string | null>(null);
	let status = $state<string>('読み込み中…');
	let hls: { destroy(): void } | null = null;

	onMount(async () => {
		channelId = new URLSearchParams(window.location.search).get('id');
		if (!channelId) {
			status = 'エラー: ?id=<channel_id> が指定されていません';
			return;
		}
		const el = video;
		if (!el) return;
		const src = `/hls/${encodeURIComponent(channelId)}/index.m3u8`;

		// Safari / iOS はネイティブ HLS 再生に対応。
		if (el.canPlayType('application/vnd.apple.mpegurl')) {
			el.src = src;
			status = '';
			return;
		}
		// それ以外は hls.js を遅延 import (/watch のときだけバンドルを読む)。
		try {
			const Hls = (await import('hls.js')).default;
			if (Hls.isSupported()) {
				const h = new Hls({ enableWorker: true, lowLatencyMode: true });
				h.loadSource(src);
				h.attachMedia(el);
				hls = h;
				status = '';
			} else {
				status = 'このブラウザは HLS 再生に対応していません';
			}
		} catch (e) {
			status =
				'HLS プレイヤーの読み込みに失敗しました: ' + (e instanceof Error ? e.message : String(e));
		}
	});

	onDestroy(() => hls?.destroy());
</script>

<svelte:head>
	<title>PSTPlayer · 視聴{channelId ? ` (${channelId.slice(0, 8)})` : ''}</title>
</svelte:head>

<main>
	<!-- svelte-ignore a11y_media_has_caption -->
	<video bind:this={video} controls autoplay playsinline></video>
	{#if status}
		<p class="status">{status}</p>
	{/if}
</main>

<style>
	:global(html, body) {
		margin: 0;
		height: 100%;
		background: #000;
	}
	main {
		display: flex;
		align-items: center;
		justify-content: center;
		height: 100vh;
		overflow: hidden;
	}
	video {
		width: 100%;
		height: 100%;
		max-height: 100vh;
		background: #000;
	}
	.status {
		position: fixed;
		top: 1rem;
		left: 1rem;
		color: #ddd;
		font-family: sans-serif;
		font-size: 14px;
		background: rgba(0, 0, 0, 0.6);
		padding: 0.4rem 0.8rem;
		border-radius: 4px;
	}
</style>
