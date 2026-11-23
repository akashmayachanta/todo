# Web push reminders

Status: needs-triage

The browser copy (`/tasks-app/`, installable as a PWA over HTTPS) shows no reminders:
`askReminderPermission()` in `app/src/lib/stores/taskReminders.ts` returns early outside
Tauri. The Windows and Android apps schedule OS notifications locally
(`app/src-tauri/src/reminders.rs`), but a browser can't schedule one ahead of time, so
the server has to send them with Web Push.

## Sketch

- **VAPID keys**: one pair, made on first start and kept in `data/`. The public key is
  served to the page.
- **Subscriptions**: Settings has a "Reminders in this browser" switch (iOS asks for
  permission only after a tap). It calls `pushManager.subscribe()` and posts the
  result to a new `POST /push/subscribe`, stored per account and device in a new table
  (a new numbered migration). `DELETE` on sign-out. The server drops an endpoint when the
  push service answers 404/410.
- **Scheduler** in tasks-server: a loop finds open, not-deleted tasks whose `remind_at`
  is due and not yet sent, and pushes each once (the `web-push` crate). It records what
  was sent so a reminder moved by an edit or sync fires again. `remind_at` is already an
  instant, so the server needs no device zone. The rules stay in `core/src/reminder.rs`.
- **Service worker** (`app/src/service-worker.ts`): `push` shows the notification
  (plain title via `plainText`, duration as on the desktop). `notificationclick` focuses
  or opens `/tasks-app/tasks?task=…`.
- Push only to devices that subscribed, so the APK and Windows app don't get duplicates.

## Limits

- iOS: only from the installed home-screen app, iOS 16.4+.
- If the server is down at the due moment, the reminder arrives late.
