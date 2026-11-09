// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// The app's own Tauri plugin ("tasks-android" in app/src-tauri/src/android.rs):
// schedules reminders, reports whether Android will deliver them on time, and
// opens the settings that fix it, and opens the installer on a downloaded app
// update (updates.rs). A tapped reminder's task goes to Rust from here.
package dev.tasks.app

import android.app.Activity
import android.content.Intent
import android.net.Uri
import android.os.Build
import android.os.PowerManager
import android.provider.Settings
import android.webkit.WebView
import androidx.core.app.NotificationManagerCompat
import androidx.core.content.FileProvider
import java.io.File
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

@InvokeArg
class ScheduleArgs {
  var dataDir: String = ""
  var reminders: String = "[]"
}

@InvokeArg
class SettingArgs {
  var kind: String = ""
}

@InvokeArg
class InstallArgs {
  var path: String = ""
}

@TauriPlugin
class TasksPlugin(private val activity: Activity) : Plugin(activity) {
  override fun load(webView: WebView) {
    Reminders.ensureChannel(activity)
    SyncWorker.enqueue(activity)
    takeTask(activity.intent)
  }

  override fun onNewIntent(intent: Intent) {
    takeTask(intent)
  }

  private fun takeTask(intent: Intent?) {
    val id = intent?.getStringExtra(Reminders.EXTRA_TASK) ?: return
    intent.removeExtra(Reminders.EXTRA_TASK)
    Native.openTask(id)
  }

  @Command
  fun schedule(invoke: Invoke) {
    val args = invoke.parseArgs(ScheduleArgs::class.java)
    Reminders.setDataDir(activity, args.dataDir)
    Reminders.schedule(activity, args.reminders)
    invoke.resolve()
  }

  @Command
  fun health(invoke: Invoke) {
    val power = activity.getSystemService(PowerManager::class.java)
    val ret = JSObject()
    ret.put("notifications", NotificationManagerCompat.from(activity).areNotificationsEnabled())
    ret.put("exactAlarms", Reminders.canExact(activity))
    ret.put("batteryUnrestricted", power.isIgnoringBatteryOptimizations(activity.packageName))
    invoke.resolve(ret)
  }

  @Command
  fun openSetting(invoke: Invoke) {
    val kind = invoke.parseArgs(SettingArgs::class.java).kind
    val pkg = activity.packageName
    val intent = when {
      kind == "notifications" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.O ->
        Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS).putExtra(Settings.EXTRA_APP_PACKAGE, pkg)
      kind == "exactAlarms" && Build.VERSION.SDK_INT >= Build.VERSION_CODES.S ->
        Intent(Settings.ACTION_REQUEST_SCHEDULE_EXACT_ALARM, Uri.parse("package:$pkg"))
      kind == "battery" ->
        Intent(Settings.ACTION_REQUEST_IGNORE_BATTERY_OPTIMIZATIONS, Uri.parse("package:$pkg"))
      else -> null
    }
    try {
      activity.startActivity(intent ?: appDetails(pkg))
    } catch (_: Exception) {
      activity.startActivity(appDetails(pkg))
    }
    invoke.resolve()
  }

  /** Opens Android's installer on an APK in the cache (see UpdateFileProvider). */
  @Command
  fun installApk(invoke: Invoke) {
    val file = File(invoke.parseArgs(InstallArgs::class.java).path)
    if (!file.isFile) {
      invoke.reject("The downloaded update is missing")
      return
    }
    val uri = FileProvider.getUriForFile(activity, UpdateFileProvider.authority(activity), file)
    val intent = Intent(Intent.ACTION_VIEW)
      .setDataAndType(uri, "application/vnd.android.package-archive")
      .addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
    try {
      activity.startActivity(intent)
      invoke.resolve()
    } catch (e: Exception) {
      invoke.reject("Can't open the installer: ${e.message}")
    }
  }

  private fun appDetails(pkg: String) = Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:$pkg"))
}
