import { describe, expect, test } from 'vitest';
import { formatUptime, renderBodyHtml } from './format';

describe('formatUptime', () => {
	test('秒を HH:MM:SS に整形', () => {
		expect(formatUptime(0)).toBe('00:00:00');
		expect(formatUptime(3661)).toBe('01:01:01');
	});
	test('負値 / 非数は 00:00:00', () => {
		expect(formatUptime(-5)).toBe('00:00:00');
		expect(formatUptime(NaN)).toBe('00:00:00');
	});
});

describe('renderBodyHtml', () => {
	test('アンカー >>N をリンク化する', () => {
		const html = renderBodyHtml('>>123 をどうぞ');
		expect(html).toContain('class="anchor"');
		expect(html).toContain('data-anchor-from="123"');
	});

	test('通常の http(s) URL をリンク化する', () => {
		const html = renderBodyHtml('https://example.com/x');
		expect(html).toContain('href="https://example.com/x"');
		expect(html).toContain('class="external"');
	});

	test('h 抜き URL (ttp:// / ttps://) もリンク化し href は復元する', () => {
		const html = renderBodyHtml('ttps://example.com/x と ttp://foo.test');
		// 表示は投稿どおり (h 抜き) のまま
		expect(html).toContain('>ttps://example.com/x</a>');
		expect(html).toContain('>ttp://foo.test</a>');
		// href は正規の http(s) に復元
		expect(html).toContain('href="https://example.com/x"');
		expect(html).toContain('href="http://foo.test"');
	});

	test('HTML 特殊文字をエスケープする', () => {
		const html = renderBodyHtml('<script>alert(1)</script> & "q"');
		expect(html).not.toContain('<script>');
		expect(html).toContain('&lt;script&gt;');
		expect(html).toContain('&amp;');
	});
});
