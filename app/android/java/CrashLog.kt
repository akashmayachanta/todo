// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// Saves crash reports to Downloads/Tasks/, where the Files app can share them:
// there's no other way to get a log off a phone that can't reach adb.
//
// A ContentProvider, because Android starts those before any activity, and so
// before the Rust library loads. On each start it:
// - saves the crashes Android recorded since the last start (Android 11+). This
//   covers native crashes in the Rust code, with the tombstone on Android 12+,
//   and freezes (ANRs).
// - catches Kotlin/Java crashes in this run as they happen.
// Each report ends with this app's recent logcat, where Rust panics are logged
// (android.rs, install_panic_log).
package dev.tasks.app

import android.app.ActivityManager
import android.app.ApplicationExitInfo
import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.net.Uri
import android.os.Build
import android.os.Environment
import android.provider.MediaStore
import java.io.File
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import java.util.concurrent.TimeUnit

object CrashLog {
  private const val PREFS = "tasks-crash-log"
  private const val LAST_EXIT = "lastExit"
  private const val CHECKED = "checkedVersion"

  fun install(context: Context) {
    val app = context.applicationContext
    saveRecordedExits(app)
    val previous = Thread.getDefaultUncaughtExceptionHandler()
    Thread.setDefaultUncaughtExceptionHandler { thread, error ->
      try {
        save(app, "crash", "Thread: ${thread.name}\n\n${error.stackTraceToString()}")
      } catch (_: Throwable) {
      }
      previous?.uncaughtException(thread, error)
    }
  }

  private fun reasonName(reason: Int) = when (reason) {
    ApplicationExitInfo.REASON_CRASH -> "crash"
    ApplicationExitInfo.REASON_CRASH_NATIVE -> "native-crash"
    ApplicationExitInfo.REASON_ANR -> "freeze"
    ApplicationExitInfo.REASON_INITIALIZATION_FAILURE -> "init-failure"
    ApplicationExitInfo.REASON_SIGNALED -> "signaled"
    ApplicationExitInfo.REASON_EXIT_SELF -> "exit-self"
    ApplicationExitInfo.REASON_LOW_MEMORY -> "low-memory"
    ApplicationExitInfo.REASON_EXCESSIVE_RESOURCE_USAGE -> "excessive-resources"
    ApplicationExitInfo.REASON_DEPENDENCY_DIED -> "dependency-died"
    ApplicationExitInfo.REASON_PERMISSION_CHANGE -> "permission-change"
    ApplicationExitInfo.REASON_USER_REQUESTED -> "user-requested"
    ApplicationExitInfo.REASON_USER_STOPPED -> "user-stopped"
    ApplicationExitInfo.REASON_OTHER -> "other"
    else -> "reason-$reason"
  }

  /** Exits that are normal: the user closed the app, or it ended itself cleanly. */
  private fun isNormal(exit: ApplicationExitInfo) = when (exit.reason) {
    ApplicationExitInfo.REASON_USER_REQUESTED, ApplicationExitInfo.REASON_USER_STOPPED -> true
    ApplicationExitInfo.REASON_EXIT_SELF -> exit.status == 0
    else -> false
  }

  /**
   * The app's abnormal exits Android recorded since the last report, each to its
   * own file. Once per app version it also writes tasks-check-<version>.txt,
   * listing every recorded exit, so it shows the logger runs at all.
   */
  private fun saveRecordedExits(context: Context) {
    val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
    val version = versionName(context)
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) {
      if (prefs.getString(CHECKED, null) != version) {
        save(context, "check-$version", "The crash log runs. Android ${Build.VERSION.RELEASE} keeps no exit history, so only Kotlin/Java crashes are saved.", logcat = false)
        prefs.edit().putString(CHECKED, version).apply()
      }
      return
    }
    try {
      val last = prefs.getLong(LAST_EXIT, 0)
      val am = context.getSystemService(ActivityManager::class.java)
      val all = am.getHistoricalProcessExitReasons(context.packageName, 0, 20)
      if (prefs.getString(CHECKED, null) != version) {
        val list = if (all.isEmpty()) "(none recorded)" else all.joinToString("\n") {
          "${stamp(it.timestamp)}  ${reasonName(it.reason)}  status ${it.status}  ${it.description ?: ""}"
        }
        save(context, "check-$version", "The crash log runs. Exits Android recorded for this app:\n$list", logcat = false)
        prefs.edit().putString(CHECKED, version).apply()
      }
      val exits = all.filter { it.timestamp > last }
      if (exits.isEmpty()) return
      prefs.edit().putLong(LAST_EXIT, exits.maxOf { it.timestamp }).apply()
      for (exit in exits.sortedBy { it.timestamp }) {
        // Kotlin/Java crashes were already saved, with their stack, as they happened.
        if (isNormal(exit) || exit.reason == ApplicationExitInfo.REASON_CRASH) continue
        val kind = reasonName(exit.reason)
        val trace = try {
          exit.traceInputStream?.bufferedReader()?.use { it.readText() }
        } catch (_: Throwable) {
          null
        }
        save(
          context,
          kind,
          buildString {
            append("Happened: ${stamp(exit.timestamp)}\n")
            append("Description: ${exit.description}\n")
            append("Process: ${exit.processName}, reason ${reasonName(exit.reason)}, status ${exit.status}\n\n")
            append(trace ?: "(Android kept no trace for this one)")
          },
        )
      }
    } catch (e: Throwable) {
      try {
        save(context, "logger-error", e.stackTraceToString(), logcat = false)
      } catch (_: Throwable) {
      }
    }
  }

  private fun versionName(context: Context): String = try {
    context.packageManager.getPackageInfo(context.packageName, 0).versionName ?: "?"
  } catch (_: Throwable) {
    "?"
  }

  private fun stamp(ms: Long) = SimpleDateFormat("yyyy-MM-dd HH:mm:ss", Locale.US).format(Date(ms))

  /** This app's own recent log lines. An app can read its own without a permission. */
  private fun ownLogcat(): String = try {
    val p = ProcessBuilder("logcat", "-d", "-v", "time", "-t", "3000").redirectErrorStream(true).start()
    val out = p.inputStream.bufferedReader().use { it.readText() }
    p.waitFor(5, TimeUnit.SECONDS)
    out
  } catch (e: Throwable) {
    "(couldn't read logcat: ${e.message})"
  }

  private fun save(context: Context, kind: String, body: String, logcat: Boolean = true) {
    val name = "tasks-$kind-${SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())}.txt"
    val version = versionName(context)
    val text = buildString {
      append("Tasks $version, Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT}), ")
      append("${Build.MANUFACTURER} ${Build.MODEL}\n")
      append("Saved: ${stamp(System.currentTimeMillis())}\n\n")
      append(body)
      if (logcat) {
        append("\n\n──── logcat (this app) ────\n")
        append(ownLogcat())
      }
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      try {
        // Download/Tasks through MediaStore: no storage permission needed.
        val values = ContentValues().apply {
          put(MediaStore.Downloads.DISPLAY_NAME, name)
          put(MediaStore.Downloads.MIME_TYPE, "text/plain")
          put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Tasks")
        }
        val resolver = context.contentResolver
        val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values)
          ?: throw IllegalStateException("MediaStore refused the file")
        resolver.openOutputStream(uri)?.use { it.write(text.toByteArray()) }
        return
      } catch (_: Throwable) {
        // Falls through to the app's own folder.
      }
    }
    // Android 9 and older, or MediaStore failed: Android/data/dev.tasks.app/files/crash.
    val dir = File(context.getExternalFilesDir(null), "crash").apply { mkdirs() }
    File(dir, name).writeText(text)
  }
}

/** Only here to run CrashLog.install at process start (declared by android-native.mjs). */
class CrashLogProvider : ContentProvider() {
  override fun onCreate(): Boolean {
    context?.let { CrashLog.install(it) }
    return true
  }

  override fun query(uri: Uri, projection: Array<String>?, selection: String?, selectionArgs: Array<String>?, sortOrder: String?): Cursor? = null
  override fun getType(uri: Uri): String? = null
  override fun insert(uri: Uri, values: ContentValues?): Uri? = null
  override fun delete(uri: Uri, selection: String?, selectionArgs: Array<String>?): Int = 0
  override fun update(uri: Uri, values: ContentValues?, selection: String?, selectionArgs: Array<String>?): Int = 0
}
