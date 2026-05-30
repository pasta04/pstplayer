// Theme switcher. CSS variables under :root / :root[data-theme="light"].
// Persisted via localStorage so subsequent loads start with the same theme.

export type Theme = 'dark' | 'light' | 'system';

const STORAGE_KEY = 'pstplayer.theme';

export function getTheme(): Theme {
	try {
		const v = localStorage.getItem(STORAGE_KEY);
		if (v === 'light' || v === 'dark' || v === 'system') return v;
	} catch {
		/* SSR or storage disabled */
	}
	return 'system';
}

function effectiveTheme(t: Theme): 'dark' | 'light' {
	if (t !== 'system') return t;
	const mq = window.matchMedia('(prefers-color-scheme: light)');
	return mq.matches ? 'light' : 'dark';
}

export function applyTheme(t: Theme): void {
	const eff = effectiveTheme(t);
	document.documentElement.setAttribute('data-theme', eff);
}

export function setTheme(t: Theme): void {
	try {
		localStorage.setItem(STORAGE_KEY, t);
	} catch {
		/* ignore */
	}
	applyTheme(t);
}

/** Install at app startup. Also re-applies on system theme change when
 * the active theme is 'system'. Returns an unbind function. */
export function initTheme(): () => void {
	const cur = getTheme();
	applyTheme(cur);
	const mq = window.matchMedia('(prefers-color-scheme: light)');
	const onChange = () => {
		if (getTheme() === 'system') applyTheme('system');
	};
	mq.addEventListener?.('change', onChange);
	return () => mq.removeEventListener?.('change', onChange);
}
