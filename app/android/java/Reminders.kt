// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// Reminders and background sync on Android, without the app open:
// - Reminders.schedule() sets one exact alarm per upcoming reminder; ReminderReceiver
//   posts the notification. The last schedule is kept so BootReceiver can set it
//   again after a reboot, an app update, or the exact-alarm permission being granted.
// - SyncWorker syncs every 15 minutes (Android's shortest period) through Rust
//   (Native.backgroundSync), so reminders set on another device get scheduled here
//   even while the app is closed.
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
import org.json.JSONArray
import java.util.concurrent.TimeUnit

/** The Rust side (app/src-tauri/src/android.rs). */
object Native {
  init {
    System.loadLibrary("tasks_app_lib")
  }

  /** Opens a task in the page (a tapped reminder). */
  @JvmStatic external fun openTask(id: String)

  /** Syncs once; returns the reminders to schedule as JSON, or null when signed out. */
  @JvmStatic external fun backgroundSync(dataDir: String): String?

  /**
   * Snoozes or completes a task from its reminder ("snooze" or "complete"). Returns the
   * reminders to schedule as JSON, or null when the running app schedules them itself.
   */
  @JvmStatic external fun reminderAction(dataDir: String, action: String, id: String, remindAt: String): String?
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
    for (r in upcoming) {
      val pi = alarmIntent(c, r)
      try {
        if (exact) am.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, r.at, pi)
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
  }
}

/** Syncs while the app is closed, then schedules what came in. */
class SyncWorker(context: Context, params: WorkerParameters) : Worker(context, params) {
  override fun doWork(): Result {
    val dir = Reminders.dataDir(applicationContext) ?: return Result.success()
    val json = try {
      Native.backgroundSync(dir)
    } catch (_: Throwable) {
      return Result.retry()
    }
    if (json != null) Reminders.schedule(applicationContext, json)
    return Result.success()
  }

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
