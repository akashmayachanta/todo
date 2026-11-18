/**
 * Task reminders are system notifications, scheduled by the app's Rust side
 * (src-tauri/src/reminders.rs) after every edit and sync, so they fire even
 * when the app is closed. The page only asks for permission to show them.
 */
import { invoke } from '@tauri-apps/api/core';

/** Asks for notification permission (the first time a reminder is set). */
export function askReminderPermission() {
	// A browser (tasks-server's web page) shows no reminders.
	if (!('__TAURI_INTERNALS__' in window)) return;
	invoke('app_ask_notification_permission').catch(() => {});
}
