<script lang="ts">
	import { ping, resolveStreamUrl, CommandError } from '$lib/api';

	let url = $state('');
	let resolved = $state<string | null>(null);
	let error = $state<string | null>(null);
	let pingResult = $state<string | null>(null);
	let busy = $state(false);

	async function onResolve() {
		busy = true;
		error = null;
		resolved = null;
		try {
			resolved = await resolveStreamUrl(url);
		} catch (e) {
			if (e instanceof CommandError) {
				error = `${e.code}: ${e.message}`;
			} else {
				error = String(e);
			}
		} finally {
			busy = false;
		}
	}

	async function onPing() {
		try {
			pingResult = await ping();
		} catch (e) {
			pingResult = `error: ${String(e)}`;
		}
	}
</script>

<svelte:head>
	<title>PSTPlayer</title>
</svelte:head>

<main>
	<header>
		<h1>PSTPlayer</h1>
		<p class="muted">phase 1 scaffold</p>
	</header>

	<section class="panel">
		<h2>URL resolution</h2>
		<p class="hint">
			PeerCast playlist or stream URL を入力して、libmpv に渡せる実 URL を取得します。
		</p>
		<form
			onsubmit={(e) => {
				e.preventDefault();
				onResolve();
			}}
		>
			<input
				type="text"
				placeholder="http://localhost:7144/pls/0123…"
				bind:value={url}
				autocomplete="off"
				spellcheck="false"
				disabled={busy}
			/>
			<button type="submit" disabled={busy || url.length === 0}>
				{busy ? 'Resolving…' : 'Resolve'}
			</button>
		</form>

		{#if resolved}
			<div class="ok">
				<strong>Stream URL:</strong>
				<code>{resolved}</code>
			</div>
		{/if}
		{#if error}
			<div class="err">
				<strong>Error:</strong>
				{error}
			</div>
		{/if}
	</section>

	<section class="panel">
		<h2>Backend ping</h2>
		<button onclick={onPing}>ping</button>
		{#if pingResult !== null}
			<span class="ok inline">→ {pingResult}</span>
		{/if}
	</section>
</main>

<style>
	:global(body) {
		font-family:
			-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, 'Noto Sans CJK JP',
			sans-serif;
		background: #1d1f23;
		color: #e8eaed;
		margin: 0;
		padding: 0;
	}

	main {
		max-width: 720px;
		margin: 0 auto;
		padding: 1.5rem 1rem;
	}

	header {
		margin-bottom: 1.5rem;
	}

	h1 {
		font-size: 1.5rem;
		margin: 0;
	}

	.muted {
		color: #8a8d94;
		margin: 0.25rem 0 0;
		font-size: 0.85rem;
	}

	.panel {
		background: #25282d;
		border: 1px solid #34373d;
		border-radius: 8px;
		padding: 1rem 1.1rem;
		margin-bottom: 1rem;
	}

	h2 {
		font-size: 1rem;
		margin: 0 0 0.5rem;
		color: #c6c9d0;
	}

	.hint {
		font-size: 0.82rem;
		color: #8a8d94;
		margin: 0 0 0.75rem;
	}

	form {
		display: flex;
		gap: 0.5rem;
	}

	input[type='text'] {
		flex: 1;
		background: #1d1f23;
		border: 1px solid #3a3d44;
		border-radius: 4px;
		color: inherit;
		padding: 0.5rem 0.7rem;
		font-family: inherit;
		font-size: 0.9rem;
	}

	input[type='text']:focus {
		outline: 2px solid #5b8def;
		border-color: transparent;
	}

	button {
		background: #3a3d44;
		color: inherit;
		border: 1px solid #4a4d54;
		border-radius: 4px;
		padding: 0.5rem 0.9rem;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.9rem;
	}

	button:hover:not(:disabled) {
		background: #4a4d54;
	}

	button:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.ok {
		margin-top: 0.75rem;
		color: #97e09e;
		font-size: 0.85rem;
		word-break: break-all;
	}

	.ok.inline {
		display: inline;
		margin-left: 0.6rem;
	}

	.err {
		margin-top: 0.75rem;
		color: #f08c8c;
		font-size: 0.85rem;
	}

	code {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		background: #1d1f23;
		padding: 0.1rem 0.3rem;
		border-radius: 3px;
	}
</style>
