// OS-native notifications via tauri-plugin-notification.
//
// Permission is requested lazily on the first notify() call.

import {
	isPermissionGranted,
	requestPermission,
	sendNotification,
} from '@tauri-apps/plugin-notification';

let permissionChecked = false;
let permissionGranted = false;

async function ensurePermission(): Promise<boolean> {
	if (permissionChecked) return permissionGranted;
	try {
		permissionGranted = await isPermissionGranted();
		if (!permissionGranted) {
			const result = await requestPermission();
			permissionGranted = result === 'granted';
		}
	} catch {
		permissionGranted = false;
	}
	permissionChecked = true;
	return permissionGranted;
}

/** Best-effort: notify if permission is granted, swallow failures. */
export async function notify(title: string, body: string): Promise<void> {
	const ok = await ensurePermission();
	if (!ok) return;
	try {
		sendNotification({ title, body });
	} catch {
		/* ignore */
	}
}
