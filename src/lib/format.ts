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
	// Linkify bare URLs (http/https).
	const withUrls = withAnchors.replace(
		/(https?:\/\/[\w\-.~:/?#[\]@!$&'()*+,;=%]+)/g,
		'<a class="external" href="$1" target="_blank" rel="noopener noreferrer">$1</a>',
	);
	// Convert newlines to <br>.
	return withUrls.replace(/\n/g, '<br>');
}

function escapeHtml(s: string): string {
	return s
		.replace(/&/g, '&amp;')
		.replace(/</g, '&lt;')
		.replace(/>/g, '&gt;')
		.replace(/"/g, '&quot;')
		.replace(/'/g, '&#39;');
}
