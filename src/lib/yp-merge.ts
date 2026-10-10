// YP 取得結果と前回の一覧をマージする (ハブ画面用)。
//
// `fetch_yp_sources` は失敗した YP の分を `entries` に含めず `failures` に
// 積んで返す。これをそのまま `entries` に代入すると、一時的な通信失敗の
// たびにその YP の全チャンネルが一覧から消え (全 YP 失敗なら空になり)、
// 復旧時には「全部新着」扱いでお気に入り通知まで再発火していた。
// 失敗した YP については前回取得済みの行をそのまま残す。
import type { YpEntry, YpMultiFetchOutcome } from './api';

/// 実チャンネル id か (通知行は id 空 / 全ゼロ)。pst-core の重複排除と同じ判定。
function realIdKey(e: YpEntry): string | null {
	const key = e.id.toLowerCase();
	if (!key || !/[^0]/.test(key)) return null;
	return key;
}

/// 今回の取得結果に、失敗した YP の前回分 (`prev` のうち `yp_source` が
/// 失敗ソースの行) を足して返す。失敗が無ければ今回分をそのまま返す。
/// 同じ channel_id が別 YP の今回分に含まれていれば前回分は捨てる
/// (バックエンドの YP 横断の重複排除と整合させる)。
export function mergeYpOutcome(prev: YpEntry[], outcome: YpMultiFetchOutcome): YpEntry[] {
	if (outcome.failures.length === 0) return outcome.entries;
	const failed = new Set(outcome.failures.map((f) => f.source));
	const freshIds = new Set<string>();
	for (const e of outcome.entries) {
		const k = realIdKey(e);
		if (k) freshIds.add(k);
	}
	const retained = prev.filter((e) => {
		if (!failed.has(e.yp_source)) return false;
		const k = realIdKey(e);
		return !(k && freshIds.has(k));
	});
	return retained.length === 0 ? outcome.entries : [...outcome.entries, ...retained];
}

/// ステータスバー向けの短い失敗表示文 (例: "YP 取得失敗: SP, TP")。
export function ypFailureSummary(outcome: YpMultiFetchOutcome): string {
	const names = outcome.failures.map((f) => f.source);
	return `YP 取得失敗: ${names.join(', ')}`;
}
