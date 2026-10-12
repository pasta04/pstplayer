import { describe, expect, test } from 'vitest';

import {
	autoReconnectText,
	manualReconnectDone,
	manualReconnectText,
	manualReconnectTimeoutText,
	peercastStateLabel,
} from '$lib/reconnect-status';

const pc = (status: string, isReceiving = false) => ({ status, isReceiving });

describe('peercastStateLabel', () => {
	test('PeerCastStation の状態を訳す', () => {
		expect(peercastStateLabel(pc('Receiving', true))).toBe('受信中');
		expect(peercastStateLabel(pc('Searching'))).toBe('ソース探索中');
		expect(peercastStateLabel(pc('Connecting'))).toBe('接続中');
		expect(peercastStateLabel(pc('Idle'))).toBe('待機中');
		expect(peercastStateLabel(pc('Error'))).toBe('エラー');
	});

	test('旧 admin 経路の大文字表記も同じに訳す', () => {
		expect(peercastStateLabel(pc('RECEIVE'))).toBe('受信中');
		expect(peercastStateLabel(pc('SEARCH'))).toBe('ソース探索中');
	});

	test('知らない値はそのまま、空なら受信フラグで判断、取れなければ null', () => {
		expect(peercastStateLabel(pc('Something'))).toBe('Something');
		expect(peercastStateLabel(pc('', true))).toBe('受信中');
		expect(peercastStateLabel(pc(''))).toBeNull();
		expect(peercastStateLabel(null)).toBeNull();
	});
});

describe('表示文', () => {
	test('手動の再接続中は経過秒と PeerCast の状況を出す', () => {
		expect(manualReconnectText(3.7, pc('Searching'))).toBe(
			'再接続中… 3秒 (PeerCast: ソース探索中)',
		);
		expect(manualReconnectText(0, null)).toBe('再接続中… 0秒');
	});

	test('自動再接続中も PeerCast の状況を添える', () => {
		expect(
			autoReconnectText({ attempt: 2, max: 10, delaySec: 4, reason: 'eof' }, pc('Connecting')),
		).toBe('自動再接続中… 試行 2/10 (4秒後 / reason=eof) (PeerCast: 接続中)');
	});

	test('時間内に終わらなかったときの表示', () => {
		expect(manualReconnectTimeoutText(pc('Searching'))).toBe(
			'再接続できていません (PeerCast: ソース探索中)',
		);
	});
});

describe('manualReconnectDone', () => {
	test('読み込み直しと PeerCast の受信中がそろって初めて完了', () => {
		expect(manualReconnectDone(true, pc('Receiving', true))).toBe(true);
		expect(manualReconnectDone(true, pc('RECEIVE'))).toBe(true);
	});

	test('Bump 直後に古い接続が受信中と返っても、読み込み直す前は完了にしない', () => {
		expect(manualReconnectDone(false, pc('Receiving', true))).toBe(false);
	});

	test('読み込み直しても PeerCast が探索中なら未完了', () => {
		expect(manualReconnectDone(true, pc('Searching'))).toBe(false);
		expect(manualReconnectDone(true, null)).toBe(false);
	});
});
