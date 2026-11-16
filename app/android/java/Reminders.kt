// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// Reminders and background sync on Android, without the app open:
// - Reminders.schedule() sets one exact alarm per upcoming reminder; ReminderReceiver
//   posts the notification. The last schedule is kept so BootReceiver can set it
//   again after a reboot, an app update, or the exact-alarm permission being granted.
// - SyncWorker syncs every 15 minutes (Android's shortest period) through Rust
//   (Native.backgroundSync), so reminders set on another device get scheduled here
//   even while the app is closed. A 15-minute alarm (SyncAlarmReceiver) does the
//   same when Android holds the job back; BackgroundSync records each run.
// - The notification's Snooze and Complete buttons go to ReminderActionReceiver,
//   which edits the task through Rust (Native.reminderAction) without opening the app.
package dev.tasks.app

import android.app.AlarmManager
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import androidx.work.Constraints
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.ExistingWorkPolicy
import androidx.work.NetworkType
import androidx.work.OneTimeWorkRequestBuilder
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.WorkManager
import androidx.work.Worker
import androidx.work.WorkerParameters
import android.os.SystemClock
import app.tauri.plugin.JSObject
import org.json.JSONArray
import org.json.JSONObject
import java.util.concurrent.TimeUnit

/** The Rust side (app/src-tauri/src/android.rs). */
object Native {
  init {
    System.loadLibrary("tasks_app_lib")
  }

  /** Opens a task in the page (a tapped reminder). */
  @JvmStatic external fun openTask(id: String)

  /**
   * Syncs once; returns `{ok, error, reminders}` as JSON (reminders: the schedule),
   * or null when the DB can't be opened.
   */
  @JvmStatic external fun backgroundSync(dataDir: String): String?

  /**
   * Snoozes or completes a task from its reminder ("snooze" or "complete"). Returns the
   * reminders to schedule as JSON, or null when the running app schedules them itself.
   */
  @JvmStatic external fun reminderAction(dataDir: String, action: String, id: String, remindAt: String): String?

  /** Opens the page's quick add with these defaults (JSON; `{}` = the page's own). */
  @JvmStatic external fun openQuickAdd(defaults: String)

  /** Shows a view of the Tasks page: its query string, e.g. "view=today". */
  @JvmStatic external fun openView(query: String)

  /** The lists a widget can show, as JSON, or null. */
  @JvmStatic external fun widgetSources(dataDir: String): String?

  /** One list for a widget, as JSON, or null. `kind`: today, inbox, upcoming, project, section. */
  @JvmStatic external fun widgetList(dataDir: String, kind: String, id: String): String?

  /** Ticks a task. Returns the reminders to schedule when the app is closed, else null. */
  @JvmStatic external fun widgetComplete(dataDir: String, id: String): String?
}

/** `remindAt` is the task's stored reminder, which the buttons hand back to Rust. */
data class Reminder(val id: String, val title: String, val at: Long, val remindAt: String)

object Reminders {
  const val CHANNEL = "reminders"
  const val EXTRA_TASK = "dev.tasks.app.task_id"
  const val EXTRA_TITLE = "dev.tasks.app.title"
  const val EXTRA_REMIND_AT = "dev.tasks.app.remind_at"
  const val ACTION_SNOOZE = "dev.tasks.app.SNOOZE"
  const val ACTION_COMPLETE = "dev.tasks.app.COMPLETE"
  private const val PREFS = "tasks-reminders"
  private const val KEY_LIST = "scheduled"
  private const val KEY_DIR = "data_dir"

  private fun prefs(c: Context) = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  fun setDataDir(c: Context, dir: String) {
    if (dir.isNotEmpty()) prefs(c).edit().putString(KEY_DIR, dir).apply()
  }

  fun dataDir(c: Context): String? = prefs(c).getString(KEY_DIR, null)

  /** A stable id per task, for its alarm and its notification. */
  fun code(taskId: String): Int = taskId.hashCode() and 0x7fffffff

  private fun parse(json: String?): List<Reminder> {
    val out = ArrayList<Reminder>()
    try {
      val arr = JSONArray(json ?: "[]")
      for (i in 0 until arr.length()) {
        val o = arr.getJSONObject(i)
        out.add(Reminder(o.getString("id"), o.optString("title"), o.getLong("at"), o.optString("remindAt")))
      }
    } catch (_: Exception) {}
    return out
  }

  fun canExact(c: Context): Boolean {
    val am = c.getSystemService(AlarmManager::class.java)
    return Build.VERSION.SDK_INT < Build.VERSION_CODES.S || am.canScheduleExactAlarms()
  }

  private fun alarmIntent(c: Context, r: Reminder): PendingIntent {
    val intent = Intent(c, ReminderReceiver::class.java)
      .putExtra(EXTRA_TASK, r.id)
      .putExtra(EXTRA_TITLE, r.title)
      .putExtra(EXTRA_REMIND_AT, r.remindAt)
    return PendingIntent.getBroadcast(c, code(r.id), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
  }

  /** Replaces the scheduled reminders with these (JSON: [{id, title, at}], at in ms). */
  @Synchronized
  fun schedule(c: Context, json: String) {
    val am = c.getSystemService(AlarmManager::class.java)
    for (r in parse(prefs(c).getString(KEY_LIST, null))) am.cancel(alarmIntent(c, r))
    val now = System.currentTimeMillis()
    val exact = canExact(c)
    val upcoming = parse(json).filter { it.at > now }
    // An alarm clock alarm is the one kind Android never defers: setExactAndAllowWhileIdle
    // still came about a minute late with the screen off. Its "show" intent opens the app.
    val show = PendingIntent.getActivity(
      c, 0,
      Intent(c, MainActivity::class.java).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    for (r in upcoming) {
      val pi = alarmIntent(c, r)
      try {
        if (exact) am.setAlarmClock(AlarmManager.AlarmClockInfo(r.at, show), pi)
        else am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, r.at, pi)
      } catch (_: SecurityException) {
        am.setAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, r.at, pi)
      }
    }
    val keep = JSONArray()
    for (r in upcoming) {
      keep.put(org.json.JSONObject().put("id", r.id).put("title", r.title).put("at", r.at).put("remindAt", r.remindAt))
    }
    prefs(c).edit().putString(KEY_LIST, keep.toString()).apply()
  }

  /** Sets the last schedule again: alarms don't survive a reboot or an update. */
  fun restore(c: Context) {
    schedule(c, prefs(c).getString(KEY_LIST, null) ?: "[]")
  }

  fun ensureChannel(c: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
    val channel = NotificationChannel(CHANNEL, "Reminders", NotificationManager.IMPORTANCE_HIGH)
    channel.description = "Task reminders"
    c.getSystemService(NotificationManager::class.java).createNotificationChannel(channel)
  }
}

/** A reminder came due: shows it. Tapping it opens the task; it has Snooze and Complete buttons. */
class ReminderReceiver : BroadcastReceiver() {
  override fun onReceive(c: Context, intent: Intent) {
    val id = intent.getStringExtra(Reminders.EXTRA_TASK) ?: return
    val title = intent.getStringExtra(Reminders.EXTRA_TITLE) ?: ""
    val remindAt = intent.getStringExtra(Reminders.EXTRA_REMIND_AT) ?: ""
    Reminders.ensureChannel(c)
    val code = Reminders.code(id)
    val open = Intent(c, MainActivity::class.java)
      .putExtra(Reminders.EXTRA_TASK, id)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    val tap = PendingIntent.getActivity(c, code, open, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
    val notification = NotificationCompat.Builder(c, Reminders.CHANNEL)
      .setSmallIcon(R.drawable.ic_stat_reminder)
      .setColor(0xFF14532D.toInt())
      .setContentTitle(title.ifEmpty { "Task reminder" })
      .setContentText("Task reminder")
      .setPriority(NotificationCompat.PRIORITY_HIGH)
      .setCategory(NotificationCompat.CATEGORY_REMINDER)
      .setAutoCancel(true)
      .setContentIntent(tap)
    // A schedule saved before the buttons existed has no remindAt; those show without them.
    if (remindAt.isNotEmpty()) {
      notification
        .addAction(0, "Snooze", button(c, Reminders.ACTION_SNOOZE, id, remindAt))
        .addAction(0, "Complete", button(c, Reminders.ACTION_COMPLETE, id, remindAt))
    }
    try {
      NotificationManagerCompat.from(c).notify(code, notification.build())
    } catch (_: SecurityException) {
      // Notifications are off for the app.
    }
  }

  /** A button's intent. Its action keeps it apart from the alarm's and the other button's. */
  private fun button(c: Context, action: String, id: String, remindAt: String): PendingIntent {
    val intent = Intent(c, ReminderActionReceiver::class.java)
      .setAction(action)
      .putExtra(Reminders.EXTRA_TASK, id)
      .putExtra(Reminders.EXTRA_REMIND_AT, remindAt)
    return PendingIntent.getBroadcast(c, Reminders.code(id), intent, PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE)
  }
}

/**
 * A reminder's Snooze or Complete button: edits the task (Rust), dismisses the
 * notification, and when the app is closed schedules the alarms that came back
 * and runs a sync to push the change.
 */
class ReminderActionReceiver : BroadcastReceiver() {
  override fun onReceive(c: Context, intent: Intent) {
    val id = intent.getStringExtra(Reminders.EXTRA_TASK) ?: return
    val remindAt = intent.getStringExtra(Reminders.EXTRA_REMIND_AT) ?: return
    val action = when (intent.action) {
      Reminders.ACTION_SNOOZE -> "snooze"
      Reminders.ACTION_COMPLETE -> "complete"
      else -> return
    }
    NotificationManagerCompat.from(c).cancel(Reminders.code(id))
    val dir = Reminders.dataDir(c) ?: return
    val app = c.applicationContext
    val pending = goAsync()
    Thread {
      try {
        val json = Native.reminderAction(dir, action, id, remindAt)
        if (json != null) {
          Reminders.schedule(app, json)
          SyncWorker.runOnce(app)
        }
        HomeWidgets.refreshAll(app)
      } catch (_: Throwable) {
        // The task stays as it was; the reminder is gone, so the app shows it.
      } finally {
        pending.finish()
      }
    }.start()
  }
}

/** Puts the alarms back after a reboot, an app update, or exact alarms being allowed. */
class BootReceiver : BroadcastReceiver() {
  override fun onReceive(c: Context, intent: Intent) {
    Reminders.restore(c)
    SyncWorker.enqueue(c)
    SyncWorker.runOnce(c)
    BackgroundSync.armAlarm(c)
  }
}

/**
 * A sync with the app closed, from either trigger: WorkManager's job (SyncWorker)
 * or the alarm (SyncAlarmReceiver). Each run is recorded for Settings' "Background
 * sync" line (TasksPlugin.health), so it shows whether Android runs it at all and,
 * when it does, why a round failed.
 */
object BackgroundSync {
  private const val PREFS = "tasks-sync"
  private const val INTERVAL_MS = 15 * 60 * 1000L
  private const val ALARM_CODE = 0x5a1c
  /** The job and the alarm landing close together sync once. */
  private const val MIN_GAP_MS = 2 * 60 * 1000L

  private fun prefs(c: Context) = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  /** Runs a round. False when it should be tried again soon (the job retries). */
  fun run(c: Context, source: String): Boolean {
    val app = c.applicationContext
    val dir = Reminders.dataDir(app)
    if (dir == null) {
      record(app, source, false, "Open the app once so it can sync in the background")
      return true
    }
    synchronized(this) {
      val p = prefs(app)
      if (p.getBoolean("ok", false) && System.currentTimeMillis() - p.getLong("at", 0) < MIN_GAP_MS) return true
      val json = try {
        Native.backgroundSync(dir)
      } catch (t: Throwable) {
        record(app, source, false, t.toString())
        return false
      }
      if (json == null) {
        record(app, source, false, "Can't open the tasks database")
        return false
      }
      try {
        val o = JSONObject(json)
        Reminders.schedule(app, o.optJSONArray("reminders")?.toString() ?: "[]")
        record(app, source, o.optBoolean("ok"), if (o.isNull("error")) null else o.optString("error"))
      } catch (e: Exception) {
        record(app, source, false, e.toString())
      }
      // What came in shows on the home screen too.
      HomeWidgets.refreshAll(app)
      return true
    }
  }

  private fun record(c: Context, source: String, ok: Boolean, error: String?) {
    prefs(c).edit()
      .putLong("at", System.currentTimeMillis())
      .putBoolean("ok", ok)
      .putString("error", error)
      .putString("source", source)
      .apply()
  }

  /** The last run: `{at (ms), ok, error, source}`, or null when there was none. */
  fun last(c: Context): JSObject? {
    val p = prefs(c)
    val at = p.getLong("at", 0)
    if (at == 0L) return null
    val o = JSObject()
    o.put("at", at)
    o.put("ok", p.getBoolean("ok", false))
    o.put("error", p.getString("error", null))
    o.put("source", p.getString("source", ""))
    return o
  }

  /**
   * The next alarm, 15 minutes on. An alarm that may fire while the phone is idle
   * runs even when Android holds WorkManager's job back (Doze, Battery Saver).
   * Exact when exact alarms are allowed (as for reminders), else roughly then.
   */
  fun armAlarm(c: Context) {
    val am = c.getSystemService(AlarmManager::class.java)
    val pi = PendingIntent.getBroadcast(
      c,
      ALARM_CODE,
      Intent(c, SyncAlarmReceiver::class.java),
      PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
    )
    val at = SystemClock.elapsedRealtime() + INTERVAL_MS
    try {
      if (Reminders.canExact(c)) am.setExactAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
      else am.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
    } catch (_: SecurityException) {
      am.setAndAllowWhileIdle(AlarmManager.ELAPSED_REALTIME_WAKEUP, at, pi)
    }
  }
}

/** The 15-minute sync alarm: arms the next one, then syncs. */
class SyncAlarmReceiver : BroadcastReceiver() {
  override fun onReceive(c: Context, intent: Intent) {
    BackgroundSync.armAlarm(c)
    val pending = goAsync()
    Thread {
      try {
        BackgroundSync.run(c, "alarm")
      } catch (_: Throwable) {
      } finally {
        pending.finish()
      }
    }.start()
  }
}

/** WorkManager's sync job: every 15 minutes while online, and once after a change made with the app closed. */
class SyncWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork(): Result =
    if (BackgroundSync.run(applicationContext, "job")) Result.success() else Result.retry()

  companion object {
    private const val PERIODIC = "tasks-sync"
    private const val NOW = "tasks-sync-now"
    private val online = Constraints.Builder().setRequiredNetworkType(NetworkType.CONNECTED).build()

    fun enqueue(c: Context) {
      val req = PeriodicWorkRequestBuilder<SyncWorker>(15, TimeUnit.MINUTES).setConstraints(online).build()
      WorkManager.getInstance(c).enqueueUniquePeriodicWork(PERIODIC, ExistingPeriodicWorkPolicy.KEEP, req)
    }

    fun runOnce(c: Context) {
      val req = OneTimeWorkRequestBuilder<SyncWorker>().setConstraints(online).build()
      WorkManager.getInstance(c).enqueueUniqueWork(NOW, ExistingWorkPolicy.REPLACE, req)
    }
  }
}
