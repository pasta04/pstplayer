<script lang="ts">
	import { onMount } from 'svelte';
	import { emit } from '@tauri-apps/api/event';
	import {
		CommandError,
		clearHistory,
		configFilePath,
		getConfig,
		getHistory,
		pushRecentHost,
		recordingTargetDir,
		setConfig,
		snapshotTargetDir,
		type Config,
		type FavoriteRule,
		type HistoryEntry,
	} from '$lib/api';
	import { applyTheme, getTheme, setTheme, type Theme } from '$lib/theme';
	import { HOTKEY_DEFS, bindingFromEvent, detectConflicts, type HotkeyDef } from '$lib/shortcuts';

	let cfg = $state<Config | null>(null);
	let tab = $state<'general' | 'peercast' | 'bbs' | 'player' | 'favorites' | 'hotkeys' | 'history'>(
		'general',
	);
	let saving = $state(false);

	// ── ホットキー編集状態 ──────────────────────────────────────
	// 編集中のキャプチャ対象 (action id)。null なら待機中ではない。
	let capturingId = $state<string | null>(null);

	function currentBinding(def: HotkeyDef): string {
		if (!cfg) return def.defaultBinding;
		const hk = (cfg.hotkeys ?? {}) as Record<string, string>;
		return hk[def.id] ?? def.defaultBinding;
	}

	function setBinding(id: string, binding: string) {
		if (!cfg) return;
		const hk = { ...((cfg.hotkeys ?? {}) as Record<string, string>) };
		hk[id] = binding;
		cfg.hotkeys = hk;
	}

	function resetBinding(id: string) {
		if (!cfg) return;
		const hk = { ...((cfg.hotkeys ?? {}) as Record<string, string>) };
		delete hk[id];
		cfg.hotkeys = hk;
	}

	function disableBinding(id: string) {
		// 空文字列を入れて「割当無し」として記録。
		setBinding(id, '');
	}

	function startCapture(id: string) {
		capturingId = id;
	}

	function onCaptureKey(e: KeyboardEvent) {
		if (!capturingId) return;
		// Esc でキャプチャをキャンセル。
		if (e.key === 'Escape') {
			e.preventDefault();
			capturingId = null;
			return;
		}
		const s = bindingFromEvent(e);
		if (!s) return; // 単独 Modifier だけは無視
		e.preventDefault();
		setBinding(capturingId, s);
		capturingId = null;
	}

	const conflicts = $derived.by(() => {
		if (!cfg) return {} as Record<string, string[]>;
		const hk: Record<string, string> = {};
		const stored = (cfg.hotkeys ?? {}) as Record<string, string>;
		for (const def of HOTKEY_DEFS) hk[def.id] = stored[def.id] ?? def.defaultBinding;
		return detectConflicts(hk);
	});
	let message = $state<string | null>(null);
	let configPath = $state<string>('');
	let theme = $state<Theme>('system');
	let history = $state<HistoryEntry[]>([]);
	let snapshotPreview = $state<string>('');
	let recordingPreview = $state<string>('');

	onMount(async () => {
		applyTheme(getTheme()); // settings window also reflects the chosen theme
		theme = getTheme();
		try {
			cfg = await getConfig();
			configPath = await configFilePath();
			history = await getHistory();
			snapshotPreview = await snapshotTargetDir();
			recordingPreview = await recordingTargetDir();
		} catch (e) {
			message = errMsg(e);
		}
	});

	async function refreshSnapshotPreview() {
		try {
			snapshotPreview = await snapshotTargetDir();
		} catch {
			/* ignore */
		}
	}

	async function refreshRecordingPreview() {
		try {
			recordingPreview = await recordingTargetDir();
		} catch {
			/* ignore */
		}
	}

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
			// MRU: 直前に入力された host:port を最近使ったホストに push。
			// (host=空 / port 不正は backend 側で no-op)
			if (cfg.peercast.host) {
				await pushRecentHost(cfg.peercast.host, cfg.peercast.port).catch(() => undefined);
				cfg = await getConfig();
			}
			await emit('config:saved');
			message = '保存しました。';
		} catch (e) {
			message = errMsg(e);
		} finally {
			saving = false;
		}
	}

	// ── お気に入りルール ────────────────────────────────────────
	const emptyRule = (): FavoriteRule => ({
		name: '',
		channel_name: '',
		genre: '',
		desc: '',
		comment: '',
		pin_top: false,
		auto_record: false,
		color: '',
		background: '',
		text_color: '',
		action: 'show',
	});

	function ensureFavorites(): FavoriteRule[] {
		if (!cfg) return [];
		if (!cfg.favorites) cfg.favorites = { rules: [] };
		if (!cfg.favorites.rules) cfg.favorites.rules = [];
		return cfg.favorites.rules;
	}

	function addRule() {
		ensureFavorites().push(emptyRule());
		cfg = { ...cfg! };
	}

	function deleteRule(i: number) {
		const rules = ensureFavorites();
		rules.splice(i, 1);
		cfg = { ...cfg! };
	}

	function moveRule(i: number, dir: -1 | 1) {
		const rules = ensureFavorites();
		const j = i + dir;
		if (j < 0 || j >= rules.length) return;
		[rules[i], rules[j]] = [rules[j], rules[i]];
		cfg = { ...cfg! };
	}

	function applyRecentHost(entry: string) {
		if (!cfg) return;
		// entry は "host:port" 形式。IPv6 は host:port 表記が曖昧なので
		// 最後の : で分割する。
		const idx = entry.lastIndexOf(':');
		if (idx < 0) return;
		const host = entry.slice(0, idx);
		const port = Number(entry.slice(idx + 1));
		if (!host || !Number.isFinite(port) || port < 1 || port > 65535) return;
		cfg.peercast.host = host;
		cfg.peercast.port = port;
	}

	function errMsg(e: unknown): string {
		if (e instanceof CommandError) return `${e.code}: ${e.message}`;
		return String(e);
	}
</script>

<svelte:head>
	<title>PSTPlayer · 設定</title>
</svelte:head>

<svelte:window onkeydown={onCaptureKey} />

<main>
	{#if cfg}
		<nav>
			<button class:active={tab === 'general'} onclick={() => (tab = 'general')}>一般</button>
			<button class:active={tab === 'peercast'} onclick={() => (tab = 'peercast')}>PeerCast</button>
			<button class:active={tab === 'bbs'} onclick={() => (tab = 'bbs')}>BBS</button>
			<button class:active={tab === 'player'} onclick={() => (tab = 'player')}>プレイヤー</button>
			<button class:active={tab === 'favorites'} onclick={() => (tab = 'favorites')}>
				お気に入り
			</button>
			<button class:active={tab === 'hotkeys'} onclick={() => (tab = 'hotkeys')}>
				ショートカット
			</button>
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
				<label>
					YP index.txt URL
					<input
						type="url"
						bind:value={cfg.peercast.ypUrl}
						placeholder="http://yp.example.invalid/index.txt"
					/>
				</label>
				<p class="hint small muted">
					YP (Yellow Page) チャンネル一覧の取得先。空にすると YP 機能は無効です。
				</p>
				{#if cfg.peercast.recentHosts && cfg.peercast.recentHosts.length > 0}
					<fieldset>
						<legend>最近使ったホスト</legend>
						<ul class="recent-hosts">
							{#each cfg.peercast.recentHosts as entry (entry)}
								<li>
									<button
										type="button"
										class="recent-host"
										onclick={() => applyRecentHost(entry)}
										title="このホストに切り替え (保存して反映)"
									>
										{entry}
									</button>
								</li>
							{/each}
						</ul>
						<p class="hint small muted">
							クリックで上記のホスト / ポート欄に反映されます。保存ボタンで確定。
						</p>
					</fieldset>
				{/if}
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
					/ on*属性などは除去されます。
				</p>
				<label>
					書き込み欄の送信キー
					<select bind:value={cfg.bbs.submitKey}>
						<option value="ctrl_enter">Ctrl/Cmd+Enter で送信 (Enter は改行)</option>
						<option value="shift_enter">Shift+Enter で送信 (Enter は改行)</option>
					</select>
				</label>
				<label class="check">
					<input type="checkbox" bind:checked={cfg.bbs.autoscroll} />
					新着レス到着時に末尾へ自動スクロール (手動スクロール中は一時停止)
				</label>
			{:else if tab === 'player'}
				<label>
					初期音量 (0-100)
					<input type="number" min="0" max="100" bind:value={cfg.player.volume} />
				</label>
				<label>
					スナップショット保存先
					<input
						type="text"
						bind:value={cfg.player.snapshot_dir}
						placeholder="(空 = exe 配下の snapshot/)"
						onblur={refreshSnapshotPreview}
					/>
				</label>
				<p class="hint small muted">現在の解決先: <code class="path">{snapshotPreview}</code></p>
				<label>
					形式
					<select bind:value={cfg.player.snapshot_format}>
						<option value="png">PNG (可逆、推奨)</option>
						<option value="jpg">JPEG (品質 95)</option>
					</select>
				</label>
				<p class="hint small muted">
					F2 キーまたは配信画面の右クリック → スナップショット で撮影できます。 ファイル名は <code
						>YYYYMMDD_HHmmss_チャンネル名.png</code
					> 形式。
				</p>
				<label>
					録画の保存先
					<input
						type="text"
						bind:value={cfg.player.recording_dir}
						placeholder="(空 = exe 配下の recordings/)"
						onblur={refreshRecordingPreview}
					/>
				</label>
				<p class="hint small muted">
					現在の解決先: <code class="path">{recordingPreview}</code>
				</p>
				<label>
					録画ファイル拡張子
					<input type="text" bind:value={cfg.player.recording_ext} placeholder="(空 = flv)" />
				</label>
				<p class="hint small muted">
					配信画面の右クリック → ⏺ 録画開始 で開始、もう一度押すと停止。 libmpv の stream-record で
					**再エンコードせず** 元の stream をそのまま書き出します (CPU
					負荷ほぼゼロ)。拡張子は元コンテナに合わせて指定してください (FLV 配信なら flv、mkv
					が安全な選択肢)。
				</p>
			{:else if tab === 'favorites'}
				<p class="hint small muted">
					各ルールはチャンネル一覧 (YP・PeerCast) に対して上から評価され、最初にマッチした
					ものが採用されます。フィールドは部分一致 (大文字小文字無視) で、空欄はワイルド
					カードです。複数フィールドを書くと AND 条件。
				</p>
				<table class="favorites">
					<thead>
						<tr>
							<th>名前</th>
							<th>チャンネル名</th>
							<th>ジャンル</th>
							<th>詳細</th>
							<th>コメント</th>
							<th title="上位固定">⬆</th>
							<th title="自動録画">⏺</th>
							<th title="背景色 (CSS 色)">色</th>
							<th></th>
						</tr>
					</thead>
					<tbody>
						{#each ensureFavorites() as rule, i (i)}
							<tr style={rule.color ? `background:${rule.color};` : undefined}>
								<td><input type="text" bind:value={rule.name} placeholder="メイン" /></td>
								<td><input type="text" bind:value={rule.channel_name} /></td>
								<td><input type="text" bind:value={rule.genre} /></td>
								<td><input type="text" bind:value={rule.desc} /></td>
								<td><input type="text" bind:value={rule.comment} /></td>
								<td><input type="checkbox" bind:checked={rule.pin_top} /></td>
								<td><input type="checkbox" bind:checked={rule.auto_record} /></td>
								<td>
									<input
										type="text"
										class="color"
										bind:value={rule.color}
										placeholder="#ff8a3d22"
									/>
								</td>
								<td class="ops">
									<button type="button" onclick={() => moveRule(i, -1)} title="上へ">↑</button>
									<button type="button" onclick={() => moveRule(i, 1)} title="下へ">↓</button>
									<button type="button" onclick={() => deleteRule(i)} title="削除">×</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
				<p>
					<button type="button" onclick={addRule}>＋ ルール追加</button>
				</p>
			{:else if tab === 'hotkeys'}
				<p class="hint small muted">
					各行の「変更」を押した後、割り当てたいキーを押すとそのまま記録されます (Esc
					でキャンセル)。Ctrl+1〜9 (サイズ) / Alt+1〜7 (アスペクト比) / Esc (全画面解除)
					はカスタマイズ不可です。保存ボタンで反映されます。
				</p>
				<table class="hotkeys">
					<thead>
						<tr>
							<th>動作</th>
							<th>キー</th>
							<th>操作</th>
						</tr>
					</thead>
					<tbody>
						{#each HOTKEY_DEFS as def (def.id)}
							{@const cb = currentBinding(def)}
							{@const cf = conflicts[def.id]}
							<tr class:conflict={cf}>
								<td>{def.label}</td>
								<td class="key-cell">
									{#if capturingId === def.id}
										<span class="capturing">⌨ 待機中…</span>
									{:else if cb}
										<code class="key">{cb}</code>
									{:else}
										<span class="muted small">(無し)</span>
									{/if}
									{#if cf}
										<span class="conflict-note">⚠ 衝突: {cf.join(', ')}</span>
									{/if}
								</td>
								<td class="ops">
									<button type="button" onclick={() => startCapture(def.id)}>
										{capturingId === def.id ? '…キャプ中' : '変更'}
									</button>
									<button type="button" onclick={() => disableBinding(def.id)}>無効</button>
									<button type="button" onclick={() => resetBinding(def.id)}>初期</button>
								</td>
							</tr>
						{/each}
					</tbody>
				</table>
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

	label.check {
		display: flex;
		grid-template-columns: none;
		gap: 0.5rem;
	}

	label.check input {
		width: auto;
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

	.recent-hosts {
		list-style: none;
		padding: 0;
		margin: 0.3rem 0 0;
		display: flex;
		flex-wrap: wrap;
		gap: 0.4rem;
	}

	.recent-host {
		background: var(--bg-input);
		border: 1px solid var(--border);
		color: inherit;
		padding: 0.2rem 0.6rem;
		border-radius: 4px;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.78rem;
		cursor: pointer;
	}

	.recent-host:hover {
		background: var(--bg-elev);
		border-color: var(--border-strong);
	}

	table.favorites {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.82rem;
	}
	table.favorites th,
	table.favorites td {
		text-align: left;
		padding: 0.25rem 0.3rem;
		border-bottom: 1px solid var(--border);
		vertical-align: middle;
	}
	table.favorites th {
		font-size: 0.72rem;
		color: var(--fg-muted);
		font-weight: 600;
	}
	table.favorites input[type='text'] {
		width: 100%;
		min-width: 5rem;
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.15rem 0.35rem;
		font-family: inherit;
		font-size: 0.78rem;
	}
	table.favorites input.color {
		min-width: 6rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
	}
	table.favorites .ops button {
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.15rem 0.35rem;
		margin-right: 0.15rem;
		font-size: 0.78rem;
		cursor: pointer;
	}

	table.hotkeys {
		width: 100%;
		border-collapse: collapse;
		font-size: 0.85rem;
	}
	table.hotkeys th,
	table.hotkeys td {
		text-align: left;
		padding: 0.35rem 0.45rem;
		border-bottom: 1px solid var(--border);
		vertical-align: middle;
	}
	table.hotkeys th {
		font-size: 0.78rem;
		color: var(--fg-muted);
		font-weight: 600;
	}
	table.hotkeys tr.conflict td {
		background: color-mix(in srgb, var(--err) 14%, transparent);
	}
	table.hotkeys .key {
		background: var(--bg-input);
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.1rem 0.4rem;
		font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
		font-size: 0.78rem;
	}
	table.hotkeys .capturing {
		color: var(--accent);
		font-weight: 600;
	}
	table.hotkeys .conflict-note {
		display: block;
		color: var(--err);
		font-size: 0.7rem;
		margin-top: 0.15rem;
	}
	table.hotkeys .ops {
		display: flex;
		gap: 0.3rem;
		white-space: nowrap;
	}
	table.hotkeys .ops button {
		background: var(--bg-input);
		color: inherit;
		border: 1px solid var(--border);
		border-radius: 3px;
		padding: 0.18rem 0.5rem;
		font-size: 0.78rem;
		cursor: pointer;
	}
	table.hotkeys .ops button:hover {
		border-color: var(--border-strong);
	}
</style>
