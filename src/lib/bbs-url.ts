// コンタクト URL の正規化ヘルパー (デスクトップ viewer とブラウザ視聴
// ページで共通)。YP のコンタクト URL には末尾にブラウザ用の読み出し範囲
// (/l50, /l30, /501-1000 等) が付くことが多く、そのまま API に渡すと
// classify が拒否する (実機 QA: "not a supported BBS URL")。

/// コンタクト URL がスレッドを指す場合はそのスレッド key を、板 (掲示板)
/// URL の場合は null を返す。read.cgi / rawmode.cgi / dat 形式、または
/// したらば短縮スレ形式 (cat/board/key の 3 階層) をスレッドとみなす。
/// 板 URL (cat/board の 2 階層 / 2ch の host/board) は null。
export function threadKeyFromContact(url: string): string | null {
	const hasCgi = /\/(?:read|rawmode|write)\.cgi\//.test(url) || /\/dat\/\d+\.dat/.test(url);
	if (hasCgi) {
		const m = url.match(/.*\/(\d+)(?:\/[^/]*)?\/?$/);
		return m ? m[1] : null;
	}
	const sm = url.match(/^https?:\/\/jbbs\.shitaraba\.net\/[^/]+\/[^/]+\/(\d+)\/?$/);
	if (sm) return sm[1];
	return null; // 板 URL
}

/// スレッド URL の末尾範囲指定 (/l50 等) を落として `/{key}/` に畳む。
export function normalizeThreadUrl(url: string, key: string): string {
	const idx = url.lastIndexOf(`/${key}`);
	if (idx < 0) return url;
	return `${url.slice(0, idx)}/${key}/`;
}
