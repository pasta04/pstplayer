// PSTPlayer Web — 単一の vanilla JS。Svelte は使わない (PWA としてサイズを
// 抑えるため + 依存無しでサーバから配信したいため)。HLS は <video> の
// src に .m3u8 を流すだけ: Safari (iPad/iPhone/macOS) はネイティブ、
// それ以外は hls.js がインストールされているならフォールバック。

const $ = (id) => document.getElementById(id);

const refs = {
	refresh: $('refresh'),
	modeToggle: $('mode-toggle'),
	status: $('status'),
	channels: $('channels'),
	channelsTbody: $('channels').querySelector('tbody'),
	error: $('error'),
	playerSection: $('player-section'),
	channelsSection: $('channels-section'),
	gridSection: $('grid-section'),
	grid: $('grid'),
	gridEmpty: $('grid-empty'),
	gridStopAll: $('grid-stop-all'),
	back: $('back'),
	player: $('player'),
	playerError: $('player-error'),
	currentTitle: $('current-title'),
	record: $('record'),
	recordInfo: $('record-info'),
};

let currentChannelId = null;
let currentChannelName = '';
let favorites = [];

// 表示モード:
//   'list'   = チャンネル一覧 (既存)。クリックで単独再生へ。
//   'single' = 単独再生中。`<video id="player">` がアクティブ。
//   'grid'   = グリッド表示。複数 <video> タイル + 下に一覧 (タップで追加)。
// 'single' は 'list' のサブ状態として扱う (back で 'list' に戻る)。
let mode = 'list';

// グリッドのタイル群。`channel_id -> Tile` 。タイル間で同 ch 重複追加は無効。
const gridTiles = new Map();
// 最後にユーザが「フォーカス」したタイル (= unmute 対象)。1 本だけ unmute。
let focusedTileId = null;

function ruleMatches(rule, t) {
	const part = (needle, hay) => {
		const n = (needle ?? '').trim();
		if (!n) return true;
		return (hay ?? '').toLowerCase().includes(n.toLowerCase());
	};
	return (
		part(rule.channel_name, t.name) &&
		part(rule.genre, t.genre) &&
		part(rule.desc, t.desc) &&
		part(rule.comment, t.comment)
	);
}

function firstFavoriteMatch(t) {
	for (const r of favorites) if (ruleMatches(r, t)) return r;
	return null;
}

async function loadFavorites() {
	try {
		const resp = await fetch('/api/favorites');
		if (!resp.ok) return;
		const body = await resp.json();
		favorites = body.rules ?? [];
	} catch {
		favorites = [];
	}
}

async function loadChannels() {
	refs.status.hidden = false;
	refs.status.textContent = '読み込み中…';
	refs.channels.hidden = true;
	refs.error.hidden = true;
	await loadFavorites();
	try {
		const resp = await fetch('/api/channels');
		const body = await resp.json();
		if (!resp.ok) throw new Error(body.message ?? `HTTP ${resp.status}`);
		renderChannels(body.channels ?? []);
		refs.status.hidden = true;
		refs.channels.hidden = false;
	} catch (e) {
		refs.status.hidden = true;
		refs.error.hidden = false;
		refs.error.textContent = `⚠ ${e.message || e}`;
	}
}

function renderChannels(channels) {
	const tbody = refs.channelsTbody;
	tbody.innerHTML = '';
	const annotated = channels
		.map((ch) => {
			const info = ch.info ?? {};
			const fav = firstFavoriteMatch({
				name: info.name ?? '',
				genre: info.genre ?? '',
				desc: info.desc ?? '',
				comment: info.comment ?? '',
			});
			return { ch, info, fav };
		})
		// action = 'ignore' / 'block' は一覧から外す (favorites の挙動と整合)
		.filter(({ fav }) => !fav?.action || fav.action === 'show');
	annotated.sort((a, b) => {
		// pin_top のお気に入りは最上位に固定。次に listeners 降順。
		const pa = a.fav?.pin_top ? 1 : 0;
		const pb = b.fav?.pin_top ? 1 : 0;
		if (pa !== pb) return pb - pa;
		const la = a.ch.status?.localDirects ?? 0;
		const lb = b.ch.status?.localDirects ?? 0;
		return lb - la;
	});
	for (const { ch, info, fav } of annotated) {
		const tr = document.createElement('tr');
		const status = ch.status ?? {};
		const bg = fav?.background || fav?.color || '';
		if (bg) tr.style.background = bg;
		if (fav?.text_color) tr.style.color = fav.text_color;
		if (fav?.pin_top) tr.classList.add('pinned');
		tr.innerHTML = `
			<td class="num">${status.localDirects ?? 0}</td>
			<td>${fav ? '<span class="fav-mark">★</span>' : ''}${escapeHtml(info.name || '(unnamed)')}</td>
			<td>${escapeHtml(info.genre || '')}</td>
			<td class="num">${info.bitrate ?? 0}</td>
			<td>${escapeHtml(info.contentType || '')}</td>
		`;
		tr.title = fav
			? `★ ${fav.name || 'お気に入り'}${fav.auto_record ? ' / 自動録画' : ''}`
			: info.desc || '';
		tr.addEventListener('click', () => handleChannelClick(ch, info, fav));
		tbody.appendChild(tr);
	}
	if (annotated.length === 0) {
		const tr = document.createElement('tr');
		tr.innerHTML = `<td colspan="5" class="muted small">視聴可能なチャンネルがありません。</td>`;
		tbody.appendChild(tr);
	}
}

// 直前に attach した Hls インスタンス (戻る時に destroy する)。
let currentHls = null;

async function openChannel(id, name) {
	currentChannelId = id;
	currentChannelName = name || '';
	refs.currentTitle.textContent = name || id;
	refs.playerError.hidden = true;
	setMode('single');
	await syncRecordStatus();
	maybeAutoRecord({ name: name || '', genre: '', desc: '', comment: '' });
	disposeHls();
	// HLS プロキシ経由で <video> に流す。Safari (iOS / macOS) は m3u8
	// をネイティブで再生できるためそのまま src 指定。Android Chrome /
	// デスクトップ Chrome / Firefox はネイティブ非対応なので vendor の
	// hls.js (defer ロード済み) を使う。
	const url = `/hls/${encodeURIComponent(id)}.m3u8`;
	if (canPlayHls()) {
		refs.player.src = url;
		refs.player.play().catch(() => undefined);
	} else if (window.Hls && window.Hls.isSupported()) {
		const hls = new window.Hls({ enableWorker: true, lowLatencyMode: false });
		hls.loadSource(url);
		hls.attachMedia(refs.player);
		hls.on(window.Hls.Events.MANIFEST_PARSED, () => {
			refs.player.play().catch(() => undefined);
		});
		hls.on(window.Hls.Events.ERROR, (_evt, data) => {
			if (data.fatal) {
				refs.playerError.hidden = false;
				refs.playerError.textContent = `再生エラー: ${data.type} / ${data.details}`;
			}
		});
		currentHls = hls;
	} else {
		refs.playerError.hidden = false;
		refs.playerError.textContent =
			'このブラウザは HLS 再生に対応していません (hls.js も利用不可)。';
	}
}

function disposeHls() {
	if (currentHls) {
		try {
			currentHls.destroy();
		} catch {
			/* ignore */
		}
		currentHls = null;
	}
}

function canPlayHls() {
	const v = document.createElement('video');
	return Boolean(
		v.canPlayType('application/vnd.apple.mpegurl') || v.canPlayType('application/x-mpegURL'),
	);
}

function backToList() {
	disposeHls();
	refs.player.pause();
	refs.player.removeAttribute('src');
	refs.player.load();
	// 'single' → 'list' or 'grid' のどちらか直近のモードに戻る。グリッドに
	// タイルがあるならグリッドに戻る、無ければ一覧に戻る。
	setMode(gridTiles.size > 0 ? 'grid' : 'list');
}

function escapeHtml(s) {
	return String(s ?? '')
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

// ===== モード管理 =====

function setMode(m) {
	mode = m;
	document.body.dataset.grid = m === 'grid' ? 'on' : 'off';
	refs.gridSection.hidden = m !== 'grid';
	refs.playerSection.hidden = m !== 'single';
	// 'single' モードのみ一覧を畳む。'grid' では下に一覧を出す (= 追加 UI)。
	refs.channelsSection.hidden = m === 'single';
	if (m === 'grid') {
		refs.modeToggle.textContent = '📋 一覧';
		refs.modeToggle.title = '一覧モードに切替';
		refs.modeToggle.classList.add('active');
	} else {
		refs.modeToggle.textContent = '⊞ グリッド';
		refs.modeToggle.title = 'グリッド表示に切替';
		refs.modeToggle.classList.remove('active');
	}
	updateGridEmpty();
}

function toggleMode() {
	if (mode === 'grid') {
		setMode('list');
	} else {
		// 'single' のときは一旦 player を畳んでから grid へ。
		if (mode === 'single') {
			disposeHls();
			refs.player.pause();
			refs.player.removeAttribute('src');
			refs.player.load();
		}
		setMode('grid');
	}
}

function handleChannelClick(ch, info) {
	if (mode === 'grid') {
		addToGrid(ch, info);
	} else {
		openChannel(ch.channelId, info.name || '');
	}
}

// ===== グリッドタイル管理 =====

function addToGrid(ch, info) {
	const id = ch.channelId;
	if (gridTiles.has(id)) {
		focusTile(id);
		return;
	}
	const name = info?.name || id;
	const status = ch.status ?? {};
	const fav = firstFavoriteMatch({
		name: info?.name ?? '',
		genre: info?.genre ?? '',
		desc: info?.desc ?? '',
		comment: info?.comment ?? '',
	});

	const el = document.createElement('div');
	el.className = 'tile';
	const bg = fav?.background || fav?.color || '';
	if (bg) el.style.borderColor = bg;
	if (fav?.text_color) el.style.color = fav.text_color;
	const nameHtml = `${fav ? '<span class="fav-mark">★</span>' : ''}${escapeHtml(name)}`;
	el.innerHTML = `
		<div class="tile-video-wrap">
			<video class="tile-video" autoplay muted playsinline></video>
			<div class="tile-overlay-top">
				<span class="tile-name">${escapeHtml(name)}</span>
				<button class="tile-record" type="button" title="録画開始 / 停止">●</button>
			</div>
			<button class="tile-close" type="button" title="タイルを閉じる">✕</button>
		</div>
		<div class="tile-info">
			<div class="tile-info-name">${nameHtml}</div>
			<div class="tile-info-meta">${escapeHtml(info?.genre || '')}</div>
			<div class="tile-info-meta-row">
				<span class="tile-info-meta">👤 ${status.localDirects ?? 0}</span>
				<span class="tile-info-meta">${info?.bitrate ?? 0} kbps</span>
			</div>
		</div>
	`;
	refs.grid.appendChild(el);

	const video = el.querySelector('.tile-video');
	const url = `/hls/${encodeURIComponent(id)}.m3u8`;
	const hls = playHls(video, url);

	const closeBtn = el.querySelector('.tile-close');
	closeBtn.addEventListener('click', (e) => {
		e.stopPropagation();
		removeFromGrid(id);
	});
	const recordBtn = el.querySelector('.tile-record');
	recordBtn.addEventListener('click', (e) => {
		e.stopPropagation();
		toggleTileRecord(id, name, recordBtn);
	});
	// タイル本体タップ → そのタイルを unmute (フォーカス)、他は mute。
	el.addEventListener('click', () => focusTile(id));

	gridTiles.set(id, { element: el, video, hls, name, recordBtn });
	updateGridEmpty();
	syncTileRecordButtons();
}

function removeFromGrid(id) {
	const t = gridTiles.get(id);
	if (!t) return;
	try {
		t.hls?.destroy();
	} catch {
		/* ignore */
	}
	try {
		t.video.pause();
		t.video.removeAttribute('src');
		t.video.load();
	} catch {
		/* ignore */
	}
	t.element.remove();
	gridTiles.delete(id);
	if (focusedTileId === id) focusedTileId = null;
	updateGridEmpty();
}

function focusTile(id) {
	// iOS Safari は複数タイルが同時に unmute された <video> を再生
	// できない (片方が止まる) ので、unmute は 1 本だけにする運用。
	focusedTileId = id;
	for (const [tid, t] of gridTiles) {
		const focused = tid === id;
		t.video.muted = !focused;
		t.element.classList.toggle('focused', focused);
	}
}

function updateGridEmpty() {
	const empty = gridTiles.size === 0;
	if (refs.gridEmpty) refs.gridEmpty.hidden = !empty;
	if (refs.gridStopAll) refs.gridStopAll.hidden = empty;
}

function stopAllTiles() {
	for (const id of Array.from(gridTiles.keys())) removeFromGrid(id);
}

function playHls(video, url) {
	if (canPlayHls()) {
		video.src = url;
		video.play().catch(() => undefined);
		return null;
	}
	if (window.Hls && window.Hls.isSupported()) {
		const hls = new window.Hls({ enableWorker: true, lowLatencyMode: false });
		hls.loadSource(url);
		hls.attachMedia(video);
		hls.on(window.Hls.Events.MANIFEST_PARSED, () => {
			video.play().catch(() => undefined);
		});
		hls.on(window.Hls.Events.ERROR, (_evt, data) => {
			if (data.fatal) {
				try {
					hls.destroy();
				} catch {
					/* ignore */
				}
			}
		});
		return hls;
	}
	return null;
}

async function toggleTileRecord(id, name, btn) {
	try {
		const list = (await fetchRecordingList()) ?? [];
		const me = list.find((r) => r.channel_id === id);
		if (me) {
			await fetch('/api/record/stop', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ id }),
			});
			btn.classList.remove('on');
		} else {
			const r = await fetch('/api/record/start', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ id, name }),
			});
			if (r.ok) btn.classList.add('on');
		}
	} catch {
		/* ignore */
	}
}

async function syncTileRecordButtons() {
	const list = await fetchRecordingList();
	if (!list) return;
	const ids = new Set(list.map((r) => r.channel_id));
	for (const [id, t] of gridTiles) {
		t.recordBtn.classList.toggle('on', ids.has(id));
	}
}

refs.refresh.addEventListener('click', loadChannels);
refs.back.addEventListener('click', backToList);
refs.record.addEventListener('click', toggleRecord);
refs.modeToggle.addEventListener('click', toggleMode);
refs.gridStopAll.addEventListener('click', stopAllTiles);

// 録画状態 (タイル ⏺ ボタン) を 7 秒間隔で同期。グリッドモード外も
// 含めて軽くポーリング。`/api/record/list` は安価。
setInterval(syncTileRecordButtons, 7000);

// 録画 API は複数本対応 (/api/record/list, start, stop)。
// 現在開いているチャンネルが録画中かを 1 つだけ判定する。
async function syncRecordStatus() {
	const list = await fetchRecordingList();
	if (list === null) {
		refs.record.hidden = true;
		return;
	}
	refs.record.hidden = false;
	const me = list.find((r) => r.channel_id === currentChannelId);
	applyRecordingStatus(me ?? null);
}

async function fetchRecordingList() {
	try {
		const resp = await fetch('/api/record/list');
		if (!resp.ok) return null;
		const body = await resp.json();
		return body.recordings ?? [];
	} catch {
		return null;
	}
}

function applyRecordingStatus(entry) {
	if (entry) {
		refs.record.textContent = '⏹ 録画停止';
		refs.record.classList.add('on');
		refs.recordInfo.hidden = false;
		refs.recordInfo.textContent = `録画中: ${entry.path ?? ''}`;
	} else {
		refs.record.textContent = '⏺ 録画';
		refs.record.classList.remove('on');
		refs.recordInfo.hidden = true;
	}
}

async function toggleRecord() {
	if (!currentChannelId) return;
	try {
		const list = (await fetchRecordingList()) ?? [];
		const me = list.find((r) => r.channel_id === currentChannelId);
		if (me) {
			await fetch('/api/record/stop', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ id: currentChannelId }),
			});
			applyRecordingStatus(null);
		} else {
			const r = await fetch('/api/record/start', {
				method: 'POST',
				headers: { 'content-type': 'application/json' },
				body: JSON.stringify({ id: currentChannelId, name: currentChannelName }),
			});
			const body = await r.json();
			if (!r.ok) {
				refs.playerError.hidden = false;
				refs.playerError.textContent = `録画開始失敗: ${body.message ?? r.status}`;
				return;
			}
			applyRecordingStatus(body);
		}
	} catch (e) {
		refs.playerError.hidden = false;
		refs.playerError.textContent = `録画 API エラー: ${e.message || e}`;
	}
}

async function maybeAutoRecord(target) {
	const fav = firstFavoriteMatch(target);
	if (!fav?.auto_record) return;
	// action != "show" (= ignore / block) は自動録画対象外
	if (fav.action && fav.action !== 'show') return;
	const list = (await fetchRecordingList()) ?? [];
	if (list.some((r) => r.channel_id === currentChannelId)) return;
	try {
		const r = await fetch('/api/record/start', {
			method: 'POST',
			headers: { 'content-type': 'application/json' },
			body: JSON.stringify({ id: currentChannelId, name: currentChannelName }),
		});
		const body = await r.json();
		if (r.ok) applyRecordingStatus(body);
	} catch {
		/* best-effort */
	}
}

// Service Worker 登録 (PWA の Add to Home Screen 用)。失敗しても致命的
// ではないので catch のみ。
if ('serviceWorker' in navigator) {
	navigator.serviceWorker.register('/sw.js').catch(() => undefined);
}

loadChannels();
