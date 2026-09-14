/** Format a uptime in seconds as HH:MM:SS. */
export function formatUptime(seconds: number): string {
	if (!Number.isFinite(seconds) || seconds < 0) return '00:00:00';
	const s = Math.floor(seconds);
	const h = Math.floor(s / 3600);
	const m = Math.floor((s % 3600) / 60);
	const ss = s % 60;
	return `${pad(h)}:${pad(m)}:${pad(ss)}`;
}

function pad(n: number): string {
	return n.toString().padStart(2, '0');
}

/** Convert anchor and bare URLs in a post body to HTML, escaping
 * everything else. Returns plain text safe for use with `{@html}`
 * because we escape `< > & " '` ourselves and only emit our own tags.
 */
export function renderBodyHtml(body: string): string {
	const escaped = escapeHtml(body);
	// Linkify >>N or >>N-M (data attributes let the click handler find them).
	const withAnchors = escaped.replace(/(?:&gt;){2}(\d+)(?:-(\d+))?/g, (_m, a, b) => {
		const to = b ? `data-anchor-to="${b}"` : '';
		return `<a class="anchor" data-anchor-from="${a}" ${to}>&gt;&gt;${a}${b ? `-${b}` : ''}</a>`;
	});
	// Linkify bare URLs (http/https) と「h 抜き」URL (ttp:// / ttps://)。
	// 表示テキストは投稿どおり (h 抜きのまま) にして href だけ正規化する。
	const withUrls = withAnchors.replace(
		/((?:h?ttps?):\/\/[\w\-.~:/?#[\]@!$&'()*+,;=%]+)/g,
		(m) =>
			`<a class="external" href="${hNukiToUrl(m)}" target="_blank" rel="noopener noreferrer">${m}</a>`,
	);
	// Convert newlines to <br>.
	return withUrls.replace(/\n/g, '<br>');
}

/** Render the post's ID field as a clickable link if it looks like
 * an `ID:xxxxxxxx` token. Returns HTML safe for `{@html}`. */
export function renderIdHtml(id: string): string {
	const safe = escapeHtml(id);
	if (!safe) return '';
	return `<a class="id-link" data-id-link="${safe}">ID:${safe}</a>`;
}

/** Linkify >>N and bare URLs inside an already-trusted HTML fragment.
 * Used in HTML display mode after the body has been sanitised by
 * ammonia in the Rust backend. We only touch raw text nodes, leaving
 * existing <a>/<b>/<i>/etc. alone. */
export function linkifySanitized(html: string): string {
	// Process only text outside of tags by alternating split.
	const parts = html.split(/(<[^>]+>)/g);
	for (let i = 0; i < parts.length; i++) {
		if (i % 2 === 1) continue; // tag
		let s = parts[i];
		s = s.replace(/(?:&gt;|>){2}(\d+)(?:-(\d+))?/g, (_m, a, b) => {
			const to = b ? `data-anchor-to="${b}"` : '';
			return `<a class="anchor" data-anchor-from="${a}" ${to}>&gt;&gt;${a}${b ? `-${b}` : ''}</a>`;
		});
		s = s.replace(
			/((?:h?ttps?):\/\/[\w\-.~:/?#[\]@!$&'()*+,;=%]+)/g,
			(m) =>
				`<a class="external" href="${hNukiToUrl(m)}" target="_blank" rel="noopener noreferrer">${m}</a>`,
		);
		parts[i] = s;
	}
	return parts.join('');
}

function escapeHtml(s: string): string {
	return s
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}

/** 「h 抜き」URL (BBS 文化で URL 規制回避のため先頭の h を抜く慣習。例:
 * `ttps://…` `ttp://…`) を href 用に http(s) へ復元する。表示テキストは投稿
 * どおり (h 抜きのまま) にして、リンク先だけ正規の URL にする。 */
function hNukiToUrl(u: string): string {
	if (u.startsWith('http://') || u.startsWith('https://')) return u;
	if (u.startsWith('ttp://') || u.startsWith('ttps://')) return 'h' + u;
	return u;
}
