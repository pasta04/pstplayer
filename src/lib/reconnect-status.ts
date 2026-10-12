// 視聴画面の再接続まわりの表示ロジック。
//
// 再接続ボタン (Bump) を押しても画面に何も出ず、再接続できたのか・今どう
// なっているのかが分からなかった (実機 QA)。再接続中はその旨と PeerCast 本体
// から取れる接続状況をステータスバーに出し、終わったら結果を出す。

/// PeerCast から取れたチャンネルの状態 (getChannelStatus の一部)。
export interface PeerCastState {
	status: string;
	isReceiving: boolean;
}

/// PeerCast の状態文字列を表示用に訳す。PeerCastStation の JSON-RPC は
/// `Receiving` / `Searching` 等、旧 admin 経路は大文字の `RECEIVE` 等を返す
/// ことがあるので大文字小文字・語尾の違いを吸収する。知らない値はそのまま出す。
export function peercastStateLabel(s: PeerCastState | null): string | null {
	if (!s) return null;
	const raw = s.status.trim();
	const key = raw.toLowerCase();
	if (key.startsWith('receiv')) return '受信中';
	if (key.startsWith('search')) return 'ソース探索中';
	if (key.startsWith('connect')) return '接続中';
	if (key.startsWith('idle')) return '待機中';
	if (key.startsWith('error')) return 'エラー';
	if (key.startsWith('finish')) return '終了';
	if (key.startsWith('broadcast')) return '配信中';
	if (raw) return raw;
	return s.isReceiving ? '受信中' : null;
}

function withPeerCast(text: string, pc: PeerCastState | null): string {
	const label = peercastStateLabel(pc);
	return label ? `${text} (PeerCast: ${label})` : text;
}

/// 手動の再接続 (Bump) 中の表示。
export function manualReconnectText(elapsedSec: number, pc: PeerCastState | null): string {
	return withPeerCast(`再接続中… ${Math.max(0, Math.floor(elapsedSec))}秒`, pc);
}

/// 自動再接続 (切断を検知してバックエンドが再試行している) 中の表示。
export function autoReconnectText(
	p: { attempt: number; max: number; delaySec: number; reason: string },
	pc: PeerCastState | null,
): string {
	return withPeerCast(
		`自動再接続中… 試行 ${p.attempt}/${p.max} (${p.delaySec}秒後 / reason=${p.reason})`,
		pc,
	);
}

/// 手動の再接続が終わったか。再生が読み込み直され (FILE_LOADED を Bump 後に
/// 受けた)、かつ PeerCast が受信中になっていること。Bump 直後は PeerCast が
/// まだ古い接続を「受信中」と返すことがあるので、受信中だけでは判定しない。
export function manualReconnectDone(loadedSinceBump: boolean, pc: PeerCastState | null): boolean {
	if (!loadedSinceBump) return false;
	return !!pc && (pc.isReceiving || peercastStateLabel(pc) === '受信中');
}

/// 手動の再接続を諦めて結果を出すまでの時間 (秒)。PeerCast のソース探索は
/// 数十秒かかることがあるので短くしすぎない。
export const MANUAL_RECONNECT_TIMEOUT_SEC = 60;

/// 時間内に終わらなかったときの表示。
export function manualReconnectTimeoutText(pc: PeerCastState | null): string {
	return withPeerCast('再接続できていません', pc);
}
