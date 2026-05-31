// PSTPlayer Web — 最小 Service Worker。
//
// 目的: ホーム画面追加 (PWA) を成立させること + 静的アセットをオフライン
// 時もすぐに返せること。ライブ視聴 (HLS) と API レスポンス (/api/*) は
// キャッシュ対象外 — 常にネットワーク優先。

const CACHE = 'pstplayer-web-v1';
const STATIC = ['/', '/index.html', '/app.js', '/style.css', '/manifest.webmanifest'];

self.addEventListener('install', (event) => {
	event.waitUntil(
		caches
			.open(CACHE)
			.then((c) => c.addAll(STATIC))
			.then(() => self.skipWaiting()),
	);
});

self.addEventListener('activate', (event) => {
	event.waitUntil(
		caches
			.keys()
			.then((keys) => Promise.all(keys.filter((k) => k !== CACHE).map((k) => caches.delete(k))))
			.then(() => self.clients.claim()),
	);
});

self.addEventListener('fetch', (event) => {
	const { request } = event;
	const url = new URL(request.url);
	// API / HLS は素通り (キャッシュしない、stale を返さない)。
	if (url.pathname.startsWith('/api/') || url.pathname.startsWith('/hls/')) {
		return;
	}
	if (request.method !== 'GET') return;
	event.respondWith(
		caches.match(request).then((cached) => {
			if (cached) return cached;
			return fetch(request)
				.then((resp) => {
					if (resp.ok && url.origin === self.location.origin) {
						const clone = resp.clone();
						caches.open(CACHE).then((c) => c.put(request, clone));
					}
					return resp;
				})
				.catch(() => cached || Response.error());
		}),
	);
});
