// PSTPlayer Web — 最小 Service Worker。
//
// 目的: ホーム画面追加 (PWA) を成立させること + 静的アセットをオフライン
// 時もすぐに返せること。ライブ視聴 (HLS) と API レスポンス (/api/*) は
// キャッシュ対象外 — 常にネットワーク優先。

// 静的アセットを更新したら (hls.js のバージョン上げ、app.js / style.css の
// 変更、Web マニフェストの修正、アイコン差し替え等) は CACHE のバージョン
// 番号を bump して旧キャッシュを破棄させること。bump し忘れるとユーザ側
// ブラウザは古い app.js を返し続け、新機能が動かない / 古いバグが残る。
const CACHE = 'pstplayer-web-v8';
const STATIC = [
	'/',
	'/index.html',
	'/app.js',
	'/style.css',
	'/settings.css',
	'/settings.html',
	'/settings.js',
	'/manifest.webmanifest',
	'/vendor/hls.min.js',
	// PWA アイコン (manifest 経由でブラウザがフェッチする。ホーム画面追加
	// 時にオフラインでも参照されるためキャッシュ対象)。
	'/icon-192.svg',
	'/icon-512.svg',
];

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
