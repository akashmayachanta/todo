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

  /** The app's crashes and freezes Android recorded since the last report. */
  private fun saveRecordedExits(context: Context) {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return
    try {
      val prefs = context.getSharedPreferences(PREFS, Context.MODE_PRIVATE)
      val last = prefs.getLong(LAST_EXIT, 0)
      val am = context.getSystemService(ActivityManager::class.java)
      val exits = am.getHistoricalProcessExitReasons(context.packageName, 0, 10)
        .filter { it.timestamp > last }
      if (exits.isEmpty()) return
      prefs.edit().putLong(LAST_EXIT, exits.maxOf { it.timestamp }).apply()
      // The first start after installing has the old build's exits; they're still useful.
      for (exit in exits.sortedBy { it.timestamp }) {
        val kind = when (exit.reason) {
          ApplicationExitInfo.REASON_CRASH -> "crash"
          ApplicationExitInfo.REASON_CRASH_NATIVE -> "native-crash"
          ApplicationExitInfo.REASON_ANR -> "freeze"
          else -> continue
        }
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
            append("Process: ${exit.processName}, status ${exit.status}\n\n")
            append(trace ?: "(Android kept no trace for this one)")
          },
        )
      }
    } catch (_: Throwable) {
    }
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

  private fun save(context: Context, kind: String, body: String) {
    val name = "tasks-$kind-${SimpleDateFormat("yyyyMMdd-HHmmss", Locale.US).format(Date())}.txt"
    val version = try {
      context.packageManager.getPackageInfo(context.packageName, 0).versionName
    } catch (_: Throwable) {
      "?"
    }
    val text = buildString {
      append("Tasks $version, Android ${Build.VERSION.RELEASE} (SDK ${Build.VERSION.SDK_INT}), ")
      append("${Build.MANUFACTURER} ${Build.MODEL}\n")
      append("Saved: ${stamp(System.currentTimeMillis())}\n\n")
      append(body)
      append("\n\n──── logcat (this app) ────\n")
      append(ownLogcat())
    }
    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
      // Downloads/Tasks through MediaStore: no storage permission needed.
      val values = ContentValues().apply {
        put(MediaStore.Downloads.DISPLAY_NAME, name)
        put(MediaStore.Downloads.MIME_TYPE, "text/plain")
        put(MediaStore.Downloads.RELATIVE_PATH, "${Environment.DIRECTORY_DOWNLOADS}/Tasks")
      }
      val resolver = context.contentResolver
      val uri = resolver.insert(MediaStore.Downloads.EXTERNAL_CONTENT_URI, values) ?: return
      resolver.openOutputStream(uri)?.use { it.write(text.toByteArray()) }
    } else {
      // Android 9 and older: the app's own folder, Android/data/dev.tasks.app/files/crash.
      val dir = File(context.getExternalFilesDir(null), "crash").apply { mkdirs() }
      File(dir, name).writeText(text)
    }
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
