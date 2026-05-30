<script lang="ts">
	import { onMount } from 'svelte';
	import { CommandError, configFilePath, getConfig, setConfig, type Config } from '$lib/api';

	let cfg = $state<Config | null>(null);
	let tab = $state<'general' | 'peercast' | 'bbs' | 'player'>('peercast');
	let saving = $state(false);
	let message = $state<string | null>(null);
	let configPath = $state<string>('');

	onMount(async () => {
		try {
			cfg = await getConfig();
			configPath = await configFilePath();
		} catch (e) {
			message = errMsg(e);
		}
	});

	async function save() {
		if (!cfg) return;
		saving = true;
		message = null;
		try {
			await setConfig(cfg);
			message = '保存しました。';
		} catch (e) {
			message = errMsg(e);
		} finally {
			saving = false;
		}
	}

	function errMsg(e: unknown): string {
		if (e instanceof CommandError) return `${e.code}: ${e.message}`;
		return String(e);
	}
</script>

<svelte:head>
	<title>PSTPlayer · 設定</title>
</svelte:head>

<main>
	{#if cfg}
		<nav>
			<button class:active={tab === 'general'} onclick={() => (tab = 'general')}>一般</button>
			<button class:active={tab === 'peercast'} onclick={() => (tab = 'peercast')}>PeerCast</button>
			<button class:active={tab === 'bbs'} onclick={() => (tab = 'bbs')}>BBS</button>
			<button class:active={tab === 'player'} onclick={() => (tab = 'player')}>プレイヤー</button>
		</nav>

		<section class="tab">
			{#if tab === 'general'}
				<p class="hint">設定ファイル:</p>
				<code class="path">{configPath}</code>
				<p class="hint small">直接編集も可能です。</p>
			{:else if tab === 'peercast'}
				<label>
					ホスト
					<input type="text" bind:value={cfg.peercast.host} placeholder="localhost" />
				</label>
				<label>
					ポート
					<input type="number" min="1" max="65535" bind:value={cfg.peercast.port} />
				</label>
				<fieldset>
					<legend>Basic 認証 (リモート時のみ)</legend>
					<label>
						ユーザー
						<input type="text" bind:value={cfg.peercast.authUser} />
					</label>
					<label>
						パスワード
						<input type="password" bind:value={cfg.peercast.authPass} />
					</label>
				</fieldset>
				<label>
					接続タイムアウト (秒)
					<input type="number" min="1" max="60" bind:value={cfg.peercast.timeoutSec} />
				</label>
			{:else if tab === 'bbs'}
				<label>
					デフォルト名前
					<input type="text" bind:value={cfg.bbs.defaultName} />
				</label>
				<label>
					デフォルトメール
					<input type="text" bind:value={cfg.bbs.defaultMail} placeholder="sage" />
				</label>
				<label>
					自動更新間隔 (秒)
					<input type="number" min="1" max="120" bind:value={cfg.bbs.autoRefreshSec} />
				</label>
			{:else if tab === 'player'}
				<p class="hint small">
					プレイヤー設定はまだ最小限です。スナップショット保存先などは順次実装。
				</p>
			{/if}
		</section>

		<footer>
			{#if message}
				<span class:err={message.includes(':')}>{message}</span>
			{/if}
			<span class="grow"></span>
			<button onclick={save} disabled={saving}>{saving ? '保存中…' : '保存'}</button>
		</footer>
	{:else}
		<p class="muted">設定を読み込み中…</p>
	{/if}
</main>

<style>
	:global(html, body) {
		height: 100%;
		margin: 0;
		font-family:
			-apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, Helvetica, Arial, 'Noto Sans CJK JP',
			sans-serif;
		background: #1d1f23;
		color: #e8eaed;
	}

	main {
		display: grid;
		grid-template-rows: auto 1fr auto;
		height: 100vh;
		padding: 1rem;
		box-sizing: border-box;
	}

	nav {
		display: flex;
		gap: 0.3rem;
		border-bottom: 1px solid #2c2f34;
		padding-bottom: 0.5rem;
		margin-bottom: 0.8rem;
	}

	nav button {
		background: transparent;
		color: #c6c9d0;
		border: 1px solid transparent;
		padding: 0.3rem 0.7rem;
		border-radius: 4px;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.9rem;
	}

	nav button.active {
		background: #2c2f34;
		border-color: #3a3d44;
		color: #fff;
	}

	.tab {
		overflow-y: auto;
		display: flex;
		flex-direction: column;
		gap: 0.6rem;
	}

	label {
		display: grid;
		grid-template-columns: 8rem 1fr;
		align-items: center;
		gap: 0.6rem;
		font-size: 0.9rem;
	}

	input {
		background: #14161a;
		color: inherit;
		border: 1px solid #3a3d44;
		border-radius: 3px;
		padding: 0.35rem 0.5rem;
		font-family: inherit;
		font-size: 0.9rem;
	}

	input:focus {
		outline: 2px solid #5b8def;
		border-color: transparent;
	}

	fieldset {
		border: 1px solid #2c2f34;
		border-radius: 4px;
		padding: 0.5rem 0.7rem;
		display: grid;
		gap: 0.4rem;
		margin: 0;
	}

	legend {
		font-size: 0.85rem;
		color: #8a8d94;
		padding: 0 0.3rem;
	}

	footer {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding-top: 0.8rem;
		border-top: 1px solid #2c2f34;
		font-size: 0.85rem;
	}

	footer button {
		background: #3a3d44;
		color: inherit;
		border: 1px solid #4a4d54;
		border-radius: 3px;
		padding: 0.4rem 1rem;
		cursor: pointer;
		font-family: inherit;
	}

	footer button:disabled {
		opacity: 0.6;
		cursor: default;
	}

	.grow {
		flex: 1;
	}

	.err {
		color: #f08c8c;
	}

	.muted {
		color: #8a8d94;
	}

	.small {
		font-size: 0.8rem;
	}

	.hint {
		color: #c6c9d0;
		margin: 0 0 0.3rem;
	}

	.path {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		background: #14161a;
		padding: 0.3rem 0.5rem;
		border-radius: 3px;
		display: inline-block;
		word-break: break-all;
	}
</style>
