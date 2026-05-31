// PSTPlayer Web — 単一の vanilla JS。Svelte は使わない (PWA としてサイズを
// 抑えるため + 依存無しでサーバから配信したいため)。HLS は <video> の
// src に .m3u8 を流すだけ: Safari (iPad/iPhone/macOS) はネイティブ、
// それ以外は hls.js がインストールされているならフォールバック。

const $ = (id) => document.getElementById(id);

const refs = {
	refresh: $('refresh'),
	status: $('status'),
	channels: $('channels'),
	channelsTbody: $('channels').querySelector('tbody'),
	error: $('error'),
	playerSection: $('player-section'),
	channelsSection: $('channels-section'),
	back: $('back'),
	player: $('player'),
	playerError: $('player-error'),
	currentTitle: $('current-title'),
};

async function loadChannels() {
	refs.status.hidden = false;
	refs.status.textContent = '読み込み中…';
	refs.channels.hidden = true;
	refs.error.hidden = true;
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
	const sorted = channels.slice().sort((a, b) => {
		const la = a.status?.localDirects ?? 0;
		const lb = b.status?.localDirects ?? 0;
		return lb - la;
	});
	for (const ch of sorted) {
		const tr = document.createElement('tr');
		const info = ch.info ?? {};
		const status = ch.status ?? {};
		tr.innerHTML = `
			<td class="num">${status.localDirects ?? 0}</td>
			<td>${escapeHtml(info.name || '(unnamed)')}</td>
			<td>${escapeHtml(info.genre || '')}</td>
			<td class="num">${info.bitrate ?? 0}</td>
			<td>${escapeHtml(info.contentType || '')}</td>
		`;
		tr.addEventListener('click', () => openChannel(ch.channelId, info.name || ''));
		tbody.appendChild(tr);
	}
	if (sorted.length === 0) {
		const tr = document.createElement('tr');
		tr.innerHTML = `<td colspan="5" class="muted small">視聴可能なチャンネルがありません。</td>`;
		tbody.appendChild(tr);
	}
}

// 直前に attach した Hls インスタンス (戻る時に destroy する)。
let currentHls = null;

function openChannel(id, name) {
	refs.currentTitle.textContent = name || id;
	refs.playerError.hidden = true;
	refs.channelsSection.hidden = true;
	refs.playerSection.hidden = false;
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
	refs.playerSection.hidden = true;
	refs.channelsSection.hidden = false;
}

function escapeHtml(s) {
	return String(s ?? '')
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

refs.refresh.addEventListener('click', loadChannels);
refs.back.addEventListener('click', backToList);

// Service Worker 登録 (PWA の Add to Home Screen 用)。失敗しても致命的
// ではないので catch のみ。
if ('serviceWorker' in navigator) {
	navigator.serviceWorker.register('/sw.js').catch(() => undefined);
}

loadChannels();
