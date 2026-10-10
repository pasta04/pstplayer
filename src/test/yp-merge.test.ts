// ハブの YP 一覧マージ (yp-merge.ts) の回帰テスト:
// 一時的な取得失敗で失敗 YP の行が消えない / 復旧時は今回分に置き換わる。
import { expect, test } from 'vitest';

import type { YpEntry, YpMultiFetchOutcome } from '$lib/api';
import { mergeYpOutcome, ypFailureSummary } from '$lib/yp-merge';

const entry = (id: string, yp_source: string, name = id): YpEntry => ({
	id,
	name,
	genre: '',
	desc: '',
	comment: '',
	contact_url: '',
	listeners: 0,
	relays: 0,
	bitrate: 0,
	content_type: '',
	track_artist: '',
	track_album: '',
	track_title: '',
	track_contact: '',
	name_url_encoded: '',
	tip: '',
	uptime: '',
	flag_click: '',
	flag_extra: '',
	yp_source,
});

const fail = (source: string) => ({ source, url: `http://${source}/index.txt`, error: 'timeout' });
const ids = (list: YpEntry[]) => list.map((e) => `${e.id}@${e.yp_source}`).sort();

test('失敗が無ければ今回分をそのまま返す', () => {
	const prev = [entry('a', 'SP'), entry('b', 'TP')];
	const outcome: YpMultiFetchOutcome = { entries: [entry('c', 'SP')], failures: [] };
	expect(mergeYpOutcome(prev, outcome)).toBe(outcome.entries);
});

test('失敗した YP の前回分だけを残し、成功した YP は今回分に置き換わる', () => {
	const prev = [entry('a', 'SP'), entry('b', 'SP'), entry('x', 'TP')];
	const outcome: YpMultiFetchOutcome = {
		entries: [entry('y', 'TP')],
		failures: [fail('SP')],
	};
	expect(ids(mergeYpOutcome(prev, outcome))).toEqual(
		ids([entry('y', 'TP'), entry('a', 'SP'), entry('b', 'SP')]),
	);
});

test('全 YP が失敗しても前回の一覧を維持する', () => {
	const prev = [entry('a', 'SP'), entry('x', 'TP')];
	const outcome: YpMultiFetchOutcome = { entries: [], failures: [fail('SP'), fail('TP')] };
	expect(ids(mergeYpOutcome(prev, outcome))).toEqual(ids(prev));
});

test('前回が空 (初回失敗) なら空のまま', () => {
	const outcome: YpMultiFetchOutcome = { entries: [], failures: [fail('SP')] };
	expect(mergeYpOutcome([], outcome)).toEqual([]);
});

test('同じ channel_id が他 YP の今回分にあれば前回分は捨てる (重複排除)', () => {
	const prev = [entry('AABB', 'SP')];
	const outcome: YpMultiFetchOutcome = {
		entries: [entry('aabb', 'TP')],
		failures: [fail('SP')],
	};
	expect(ids(mergeYpOutcome(prev, outcome))).toEqual(['aabb@TP']);
});

test('通知行 (id 空 / 全ゼロ) は重複排除せず失敗 YP の分を残す', () => {
	const prev = [entry('', 'SP', 'お知らせ'), entry('0000', 'SP', '告知')];
	const outcome: YpMultiFetchOutcome = {
		entries: [entry('', 'TP', 'TP のお知らせ')],
		failures: [fail('SP')],
	};
	const merged = mergeYpOutcome(prev, outcome);
	expect(merged.map((e) => e.name).sort()).toEqual(['TP のお知らせ', 'お知らせ', '告知']);
});

test('ステータスバー用の要約文', () => {
	expect(ypFailureSummary({ entries: [], failures: [fail('SP'), fail('TP')] })).toBe(
		'YP 取得失敗: SP, TP',
	);
});
