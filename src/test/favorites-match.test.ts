// お気に入りルールのマッチロジック (api.ts ruleMatches) の回帰テスト。
// pst-core::favorites::matches と同じ仕様:
// - 新形式 (pattern 非空): 対象フィールドのどれか (OR) に部分一致
// - 旧形式 (pattern 空): フィールド別パターンの AND / 空欄ワイルドカード
import { expect, test } from 'vitest';

import { ruleMatches, type FavoriteRule } from '$lib/api';

const rule = (over: Partial<FavoriteRule>): FavoriteRule => ({
	name: '',
	pattern: '',
	match_name: true,
	match_genre: false,
	match_desc: false,
	match_comment: false,
	channel_name: '',
	genre: '',
	desc: '',
	comment: '',
	pin_top: false,
	auto_record: false,
	color: '',
	background: '',
	text_color: '',
	action: 'show',
	...over,
});

const ch = (name: string, genre = '', desc = '', comment = '') => ({
	name,
	genre,
	desc,
	comment,
});

test('新形式: チェックしたフィールドのどれか (OR) に部分一致', () => {
	const r = rule({ pattern: 'rta', match_name: true, match_genre: true });
	expect(ruleMatches(r, ch('MarioRTA'))).toBe(true);
	expect(ruleMatches(r, ch('other', 'RTA'))).toBe(true);
	expect(ruleMatches(r, ch('other', 'music'))).toBe(false);
	// チェックしていないフィールドには当てない
	expect(ruleMatches(r, ch('other', '', 'rta もやる'))).toBe(false);
});

test('新形式: | 区切り OR / 大文字小文字無視', () => {
	const r = rule({ pattern: 'へたれ|inatami|VADER' });
	expect(ruleMatches(r, ch('へたれ実況'))).toBe(true);
	expect(ruleMatches(r, ch('Inatami ch'))).toBe(true);
	expect(ruleMatches(r, ch('vader'))).toBe(true);
	expect(ruleMatches(r, ch('別チャンネル'))).toBe(false);
});

test('新形式: 対象フィールドが 1 つも無ければマッチしない', () => {
	const r = rule({ pattern: 'foo', match_name: false });
	expect(ruleMatches(r, ch('foo', 'foo', 'foo', 'foo'))).toBe(false);
});

test('旧形式 (pattern 空): フィールド別 AND / 空欄ワイルドカード', () => {
	const legacy = rule({ channel_name: 'foo', genre: 'game' });
	expect(ruleMatches(legacy, ch('foo ch', 'game'))).toBe(true);
	expect(ruleMatches(legacy, ch('foo ch', 'music'))).toBe(false);
	const empty = rule({});
	expect(ruleMatches(empty, ch('anything'))).toBe(true);
});
