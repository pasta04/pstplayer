<script lang="ts">
	import { onMount } from 'svelte';
	import {
		CommandError,
		clearHistory,
		configFilePath,
		getConfig,
		getHistory,
		setConfig,
		type Config,
		type HistoryEntry,
	} from '$lib/api';
	import { applyTheme, getTheme, setTheme, type Theme } from '$lib/theme';

	let cfg = $state<Config | null>(null);
	let tab = $state<'general' | 'peercast' | 'bbs' | 'player' | 'history'>('general');
	let saving = $state(false);
	let message = $state<string | null>(null);
	let configPath = $state<string>('');
	let theme = $state<Theme>('system');
	let history = $state<HistoryEntry[]>([]);

	onMount(async () => {
		applyTheme(getTheme()); // settings window also reflects the chosen theme
		theme = getTheme();
		try {
			cfg = await getConfig();
			configPath = await configFilePath();
			history = await getHistory();
		} catch (e) {
			message = errMsg(e);
		}
	});

	async function onClearHistory() {
		if (!confirm('視聴履歴をすべて削除します。よろしいですか?')) return;
		await clearHistory();
		history = [];
	}

	function fmtDate(unix: number): string {
		if (!unix) return '';
		return new Date(unix * 1000).toLocaleString();
	}

	function onThemeChange(e: Event) {
		const v = (e.target as HTMLSelectElement).value as Theme;
		theme = v;
		setTheme(v);
	}

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
			<button class:active={tab === 'history'} onclick={() => (tab = 'history')}>履歴</button>
		</nav>

		<section class="tab">
			{#if tab === 'general'}
				<label>
					テーマ
					<select bind:value={theme} onchange={onThemeChange}>
						<option value="system">システムに合わせる</option>
						<option value="dark">ダーク</option>
						<option value="light">ライト</option>
					</select>
				</label>
				<p class="hint small muted">テーマは即時反映、ブラウザ localStorage に保存されます。</p>
				<p class="hint">設定ファイル (TOML):</p>
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
				<label>
					表示モード
					<select bind:value={cfg.bbs.displayMode}>
						<option value="plain">プレーン (タグはエスケープ)</option>
						<option value="html">HTML (ammonia で whitelist サニタイズ)</option>
					</select>
				</label>
				<p class="hint small muted">
					HTML モードは &lt;b&gt; &lt;i&gt; &lt;font color&gt; などの装飾を反映します。 script / img
					/ on*属性などは除去されます。次回起動から有効。
				</p>
			{:else if tab === 'player'}
				<p class="hint small">
					プレイヤー設定はまだ最小限です。スナップショット保存先などは順次実装。
				</p>
			{:else if tab === 'history'}
				<div class="hist-head">
					<span class="hint">最近開いたチャンネル ({history.length})</span>
					<button class="danger" onclick={onClearHistory} disabled={history.length === 0}>
						すべて削除
					</button>
				</div>
				{#if history.length === 0}
					<p class="muted small">履歴はまだありません。</p>
				{:else}
					<ul class="hist-list">
						{#each history as h}
							<li class="hist-item">
								<div class="hist-name">{h.channelName || '(no name)'}</div>
								<div class="hist-meta">
									<span class="hist-date">{fmtDate(h.lastOpenedAt)}</span>
								</div>
								<code class="hist-url">{h.url}</code>
							</li>
						{/each}
					</ul>
				{/if}
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
		background: var(--bg);
		color: var(--fg);
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
		border-bottom: 1px solid var(--border);
		padding-bottom: 0.5rem;
		margin-bottom: 0.8rem;
	}

	nav button {
		background: transparent;
		color: var(--fg-dim);
		border: 1px solid transparent;
		padding: 0.3rem 0.7rem;
		border-radius: 4px;
		cursor: pointer;
		font-family: inherit;
		font-size: 0.9rem;
	}

	nav button.active {
		background: var(--border);
		border-color: #3a3d44;
		color: var(--fg);
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

	input,
	select {
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.35rem 0.5rem;
		font-family: inherit;
		font-size: 0.9rem;
	}

	input:focus,
	select:focus {
		outline: 2px solid var(--accent);
		border-color: transparent;
	}

	fieldset {
		border: 1px solid var(--border);
		border-radius: 4px;
		padding: 0.5rem 0.7rem;
		display: grid;
		gap: 0.4rem;
		margin: 0;
	}

	legend {
		font-size: 0.85rem;
		color: var(--fg-muted);
		padding: 0 0.3rem;
	}

	footer {
		display: flex;
		align-items: center;
		gap: 0.6rem;
		padding-top: 0.8rem;
		border-top: 1px solid var(--border);
		font-size: 0.85rem;
	}

	footer button {
		background: var(--bg-elev);
		color: inherit;
		border: 1px solid var(--border-strong);
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
		color: var(--err);
	}

	.muted {
		color: var(--fg-muted);
	}

	.small {
		font-size: 0.8rem;
	}

	.hint {
		color: var(--fg-dim);
		margin: 0 0 0.3rem;
	}

	.path {
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.8rem;
		background: var(--bg-input);
		padding: 0.3rem 0.5rem;
		border-radius: 3px;
		display: inline-block;
		word-break: break-all;
	}

	.hist-head {
		display: flex;
		align-items: center;
		gap: 0.5rem;
	}
	.hist-head .hint {
		flex: 1;
	}
	.hist-head .danger {
		background: var(--bg-elev);
		color: var(--err);
		border: 1px solid var(--border-strong);
		border-radius: 3px;
		padding: 0.25rem 0.6rem;
		cursor: pointer;
		font-size: 0.8rem;
		font-family: inherit;
	}
	.hist-head .danger:disabled {
		opacity: 0.5;
		cursor: default;
	}
	.hist-list {
		list-style: none;
		padding: 0;
		margin: 0;
		display: flex;
		flex-direction: column;
		gap: 0.3rem;
	}
	.hist-item {
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.4rem 0.6rem;
		font-size: 0.82rem;
	}
	.hist-name {
		font-weight: 600;
	}
	.hist-meta {
		color: var(--fg-muted);
		font-size: 0.72rem;
		margin-top: 0.15rem;
	}
	.hist-url {
		display: block;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.72rem;
		color: var(--fg-dim);
		word-break: break-all;
		margin-top: 0.2rem;
	}
</style>
