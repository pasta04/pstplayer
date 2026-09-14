// スレッド自動選択ロジック (デスクトップ viewer とブラウザ視聴ページで
// 共通)。仕様:
// - 候補は板の subject 並び (上位優先)。「数値 key 最大 = 最新作成」では
//   なく、板の並びを尊重する
// - subject のレス数は実際と食い違うことがあるため、候補は実際に取得して
//   満レスでないことを確認してから採用する (候補数は上限付き)
// - 満レス判定は「実レス数 >= 上限」。同時書き込みで上限を超過したり、
//   掲示板固有のメッセージが追記されることがあるため == では判定しない
import { fetchThread, listThreads, threadUrlOf, type SubjectEntry } from './api';

/// 板設定 (最大レス数) が取れなかったときのフォールバック。
export const THREAD_FULL_FALLBACK = 1000;

export interface LiveThread {
	url: string;
	key: string;
}

/// 板の subject 並び (上位優先) で「満レスでない」スレを選ぶ。
/// `excludeKeys` は現スレなどの除外。`threads` を渡すと一覧の再取得を
/// 省略できる (呼び出し側が UI 用に取得済みの場合)。
export async function selectLiveThread(
	boardUrl: string,
	maxRes: number,
	excludeKeys: string[] = [],
	opts?: { threads?: SubjectEntry[]; maxCandidates?: number },
): Promise<LiveThread | null> {
	const max = maxRes > 0 ? maxRes : THREAD_FULL_FALLBACK;
	const maxCandidates = opts?.maxCandidates ?? 5;
	const threads = opts?.threads ?? (await listThreads(boardUrl));
	const exclude = new Set(excludeKeys);
	let checked = 0;
	for (const t of threads) {
		if (!t.key || exclude.has(t.key)) continue;
		// subject 上で既に満レスのものはスキップ (実取得コストの節約)。
		if (t.count >= max) continue;
		if (checked >= maxCandidates) break;
		checked += 1;
		try {
			const url = await threadUrlOf(boardUrl, t.key);
			// subject と実際は食い違うことがあるので、実取得で満レス確認。
			const [posts] = await fetchThread(url, null);
			if (posts.length < max) return { url, key: t.key };
		} catch {
			/* 取得できないスレは飛ばして次候補へ */
		}
	}
	return null;
}
