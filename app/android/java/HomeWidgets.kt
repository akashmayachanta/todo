// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// The home screen widgets and the app icon's shortcuts:
// - TasksWidget lists one list's open tasks: Today unless another was picked
//   (TasksWidgetConfig: Inbox, Upcoming, a project or a category). The lists come
//   from Rust (Native.widgetList, app/src-tauri/src/home_widget.rs). Its title opens
//   the list in the app, + adds a task to it, the list icon picks another list, a
//   task's circle ticks it and the task itself opens it.
// - AppShortcuts: the app icon's long-press menu, Add task, Search, Today and
//   Inbox, each opening that screen in the app (QuickAction). The launcher lists
//   TasksWidget under them.
// - What a tap asks the app for goes in EXTRA_VIEW / EXTRA_QUICK_ADD on
//   MainActivity's intent; TasksPlugin hands it to Rust, which hands it to the page.
// Rust calls refreshAll (through TasksPlugin) after each change; the background
// sync and the reminders' buttons call it when the app is closed; and an alarm
// just after midnight moves Today on to the new day.
package dev.tasks.app

import android.app.Activity
import android.app.AlarmManager
import android.app.AlertDialog
import android.app.PendingIntent
import android.appwidget.AppWidgetManager
import android.appwidget.AppWidgetProvider
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.widget.RemoteViews
import android.widget.RemoteViewsService
import androidx.core.content.pm.ShortcutInfoCompat
import androidx.core.content.pm.ShortcutManagerCompat
import androidx.core.graphics.drawable.IconCompat
import org.json.JSONArray
import org.json.JSONObject
import java.util.Calendar

/** A list a TasksWidget shows. `open` is its Tasks page query string; `add` the quick add's defaults (JSON). */
data class WidgetSource(val kind: String, val id: String, val name: String, val open: String, val add: String) {
  fun toJson(): String =
    JSONObject().put("kind", kind).put("id", id).put("name", name).put("open", open).put("add", add).toString()

  companion object {
    val TODAY = WidgetSource("today", "", "Today", "view=today", """{"due":"today"}""")

    /** From Rust's JSON (home_widget::sources), where `add` is an object. */
    fun fromRust(o: JSONObject) = WidgetSource(
      o.optString("kind", "today"),
      o.optString("id"),
      o.optString("name", "Today"),
      o.optString("open", "view=today"),
      o.optJSONObject("add")?.toString() ?: "{}",
    )
  }
}

/** The app icon's long-press shortcuts. */
enum class QuickAction(val label: String, val longLabel: String, val icon: Int, val view: String?, val quickAdd: String?) {
  // `{}`: the quick add takes the page's own defaults (the list being viewed).
  ADD("Add task", "Add a task", R.drawable.ic_shortcut_add, null, "{}"),
  SEARCH("Search", "Search tasks", R.drawable.ic_shortcut_search, "view=browse&focus=search", null),
  TODAY("Today", "Today", R.drawable.ic_shortcut_today, "view=today", null),
  INBOX("Inbox", "Inbox", R.drawable.ic_shortcut_inbox, "view=inbox", null),
}

object HomeWidgets {
  /** The Tasks page view to show (its query string). */
  const val EXTRA_VIEW = "dev.tasks.app.view"
  /** The quick add to open, with these defaults (JSON). */
  const val EXTRA_QUICK_ADD = "dev.tasks.app.quick_add"
  /** On TapActivity: "open" or "complete" (a task row), else it forwards to the app. */
  const val EXTRA_TAP = "dev.tasks.app.tap"
  const val ACTION_COMPLETE = "dev.tasks.app.WIDGET_COMPLETE"
  const val ACTION_REFRESH = "dev.tasks.app.WIDGET_REFRESH"
  private const val PREFS = "tasks-widgets"
  private const val MIDNIGHT_CODE = 0x7a5c

  private fun prefs(c: Context) = c.getSharedPreferences(PREFS, Context.MODE_PRIVATE)

  fun source(c: Context, widgetId: Int): WidgetSource {
    val json = prefs(c).getString("w$widgetId", null) ?: return WidgetSource.TODAY
    return try {
      val o = JSONObject(json)
      WidgetSource(o.getString("kind"), o.optString("id"), o.optString("name"), o.optString("open"), o.optString("add", "{}"))
    } catch (_: Exception) {
      WidgetSource.TODAY
    }
  }

  fun saveSource(c: Context, widgetId: Int, s: WidgetSource) {
    prefs(c).edit().putString("w$widgetId", s.toJson()).apply()
  }

  fun forget(c: Context, widgetIds: IntArray) {
    val edit = prefs(c).edit()
    for (id in widgetIds) edit.remove("w$id")
    edit.apply()
  }

  private val mutable = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) PendingIntent.FLAG_MUTABLE else 0
  private const val IMMUTABLE = PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT

  /** The app, brought to the front, asked to show a view or open the quick add. */
  fun mainIntent(c: Context, view: String? = null, quickAdd: String? = null, task: String? = null): Intent =
    Intent(c, MainActivity::class.java)
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
      .apply {
        view?.let { putExtra(EXTRA_VIEW, it) }
        quickAdd?.let { putExtra(EXTRA_QUICK_ADD, it) }
        task?.let { putExtra(Reminders.EXTRA_TASK, it) }
      }

  /** A request code per widget and button, so their PendingIntents stay apart. */
  private fun code(widgetId: Int, button: Int) = widgetId * 8 + button

  /** Reloads every list widget: its header now, its tasks through the factory. */
  fun refreshAll(c: Context) {
    val mgr = AppWidgetManager.getInstance(c)
    val ids = mgr.getAppWidgetIds(ComponentName(c, TasksWidget::class.java))
    if (ids.isEmpty()) return
    for (id in ids) update(c, mgr, id)
    mgr.notifyAppWidgetViewDataChanged(ids, R.id.tasks_widget_list)
  }

  /** Draws a list widget's frame: title, buttons, and where its tasks come from. */
  fun update(c: Context, mgr: AppWidgetManager, widgetId: Int) {
    val s = source(c, widgetId)
    val views = RemoteViews(c.packageName, R.layout.tasks_widget)
    views.setTextViewText(R.id.tasks_widget_title, s.name)
    views.setTextViewText(
      R.id.tasks_widget_empty,
      if (Reminders.dataDir(c) == null) "Open Tasks once to see your tasks here" else "Nothing to do",
    )
    val service = Intent(c, TasksWidgetService::class.java).putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
    // Each widget its own adapter: intents that differ only in extras count as one.
    service.data = Uri.parse(service.toUri(Intent.URI_INTENT_SCHEME))
    @Suppress("DEPRECATION")
    views.setRemoteAdapter(R.id.tasks_widget_list, service)
    views.setEmptyView(R.id.tasks_widget_list, R.id.tasks_widget_empty)
    // A row's fill-in says what to do: open the task or tick it (TapActivity).
    val template = PendingIntent.getActivity(c, code(widgetId, 1), Intent(c, TapActivity::class.java), PendingIntent.FLAG_UPDATE_CURRENT or mutable)
    views.setPendingIntentTemplate(R.id.tasks_widget_list, template)
    views.setOnClickPendingIntent(R.id.tasks_widget_title, PendingIntent.getActivity(c, code(widgetId, 2), mainIntent(c, view = s.open), IMMUTABLE))
    views.setOnClickPendingIntent(R.id.tasks_widget_add, PendingIntent.getActivity(c, code(widgetId, 3), mainIntent(c, quickAdd = s.add), IMMUTABLE))
    val pick = Intent(c, TasksWidgetConfig::class.java)
      .setAction(AppWidgetManager.ACTION_APPWIDGET_CONFIGURE)
      .putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId)
      .setData(Uri.parse("tasks-widget://pick/$widgetId"))
      .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
    views.setOnClickPendingIntent(R.id.tasks_widget_pick, PendingIntent.getActivity(c, code(widgetId, 4), pick, IMMUTABLE))
    mgr.updateAppWidget(widgetId, views)
  }

  /** Just after midnight, so Today shows the new day. Inexact; a minute or so late is fine. */
  fun scheduleMidnight(c: Context) {
    val next = Calendar.getInstance().apply {
      add(Calendar.DAY_OF_YEAR, 1)
      set(Calendar.HOUR_OF_DAY, 0)
      set(Calendar.MINUTE, 1)
      set(Calendar.SECOND, 0)
      set(Calendar.MILLISECOND, 0)
    }
    c.getSystemService(AlarmManager::class.java).set(AlarmManager.RTC, next.timeInMillis, midnightIntent(c))
  }

  fun cancelMidnight(c: Context) {
    c.getSystemService(AlarmManager::class.java).cancel(midnightIntent(c))
  }

  private fun midnightIntent(c: Context) =
    PendingIntent.getBroadcast(c, MIDNIGHT_CODE, Intent(c, TasksWidget::class.java).setAction(ACTION_REFRESH), IMMUTABLE)
}

/** The list widget. */
class TasksWidget : AppWidgetProvider() {
  override fun onUpdate(c: Context, mgr: AppWidgetManager, ids: IntArray) {
    for (id in ids) HomeWidgets.update(c, mgr, id)
    mgr.notifyAppWidgetViewDataChanged(ids, R.id.tasks_widget_list)
    HomeWidgets.scheduleMidnight(c)
  }

  override fun onDeleted(c: Context, ids: IntArray) {
    HomeWidgets.forget(c, ids)
  }

  override fun onDisabled(c: Context) {
    HomeWidgets.cancelMidnight(c)
  }

  override fun onReceive(c: Context, intent: Intent) {
    when (intent.action) {
      HomeWidgets.ACTION_COMPLETE -> complete(c, intent.getStringExtra(Reminders.EXTRA_TASK) ?: return)
      HomeWidgets.ACTION_REFRESH -> {
        HomeWidgets.refreshAll(c)
        HomeWidgets.scheduleMidnight(c)
      }
      else -> super.onReceive(c, intent)
    }
  }

  /** Ticks a task (Rust), then reloads the widgets; when the app is closed, also syncs. */
  private fun complete(c: Context, id: String) {
    val dir = Reminders.dataDir(c) ?: return
    val app = c.applicationContext
    val pending = goAsync()
    Thread {
      try {
        val json = Native.widgetComplete(dir, id)
        if (json != null) {
          Reminders.schedule(app, json)
          SyncWorker.runOnce(app)
        }
      } catch (_: Throwable) {
        // The task stays open; the widget still shows it.
      } finally {
        HomeWidgets.refreshAll(app)
        pending.finish()
      }
    }.start()
  }
}

class TasksWidgetService : RemoteViewsService() {
  override fun onGetViewFactory(intent: Intent): RemoteViewsFactory =
    TasksWidgetFactory(applicationContext, intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID))
}

/** One list widget's rows, loaded from Rust on a binder thread. */
class TasksWidgetFactory(private val c: Context, private val widgetId: Int) : RemoteViewsService.RemoteViewsFactory {
  private data class Row(val id: String, val title: String, val due: String, val overdue: Boolean, val priority: Int)

  private var rows: List<Row> = emptyList()

  override fun onCreate() {}

  override fun onDestroy() {}

  override fun onDataSetChanged() {
    val dir = Reminders.dataDir(c)
    if (dir == null) {
      rows = emptyList()
      return
    }
    val s = HomeWidgets.source(c, widgetId)
    // A failed read keeps what is shown.
    val json = try {
      Native.widgetList(dir, s.kind, s.id)
    } catch (_: Throwable) {
      null
    } ?: return
    try {
      val o = JSONObject(json)
      val arr = o.getJSONArray("tasks")
      rows = (0 until arr.length()).map {
        val t = arr.getJSONObject(it)
        Row(t.getString("id"), t.optString("title"), t.optString("due"), t.optBoolean("overdue"), t.optInt("priority", 4))
      }
      // A renamed (or deleted) project or category: the title follows.
      val name = o.optString("name")
      if (name.isNotEmpty() && name != s.name) {
        HomeWidgets.saveSource(c, widgetId, s.copy(name = name))
        val title = RemoteViews(c.packageName, R.layout.tasks_widget)
        title.setTextViewText(R.id.tasks_widget_title, name)
        AppWidgetManager.getInstance(c).partiallyUpdateAppWidget(widgetId, title)
      }
    } catch (_: Exception) {}
  }

  override fun getCount() = rows.size

  override fun getViewAt(position: Int): RemoteViews {
    val views = RemoteViews(c.packageName, R.layout.tasks_widget_item)
    val row = rows.getOrNull(position) ?: return views
    views.setTextViewText(R.id.tasks_widget_item_title, row.title)
    if (row.due.isEmpty()) {
      views.setViewVisibility(R.id.tasks_widget_item_due, View.GONE)
    } else {
      views.setViewVisibility(R.id.tasks_widget_item_due, View.VISIBLE)
      views.setTextViewText(R.id.tasks_widget_item_due, row.due)
      views.setTextColor(R.id.tasks_widget_item_due, c.getColor(if (row.overdue) R.color.tasks_widget_overdue else R.color.tasks_widget_due))
    }
    val ring = when (row.priority) {
      1 -> R.color.tasks_widget_p1
      2 -> R.color.tasks_widget_p2
      3 -> R.color.tasks_widget_p3
      else -> R.color.tasks_widget_p4
    }
    views.setInt(R.id.tasks_widget_check, "setColorFilter", c.getColor(ring))
    views.setOnClickFillInIntent(
      R.id.tasks_widget_row,
      Intent().putExtra(HomeWidgets.EXTRA_TAP, "open").putExtra(Reminders.EXTRA_TASK, row.id),
    )
    views.setOnClickFillInIntent(
      R.id.tasks_widget_check,
      Intent().putExtra(HomeWidgets.EXTRA_TAP, "complete").putExtra(Reminders.EXTRA_TASK, row.id),
    )
    return views
  }

  override fun getLoadingView(): RemoteViews? = null

  override fun getViewTypeCount() = 1

  override fun getItemId(position: Int) = position.toLong()

  override fun hasStableIds() = false
}

/**
 * Picks the list a widget shows: when it is added (before Android 12; from 12 on
 * it starts on Today and this is optional), from its list icon, or from the
 * launcher's Reconfigure.
 */
class TasksWidgetConfig : Activity() {
  private var widgetId = AppWidgetManager.INVALID_APPWIDGET_ID

  override fun onCreate(saved: Bundle?) {
    super.onCreate(saved)
    widgetId = intent.getIntExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, AppWidgetManager.INVALID_APPWIDGET_ID)
    if (widgetId == AppWidgetManager.INVALID_APPWIDGET_ID) {
      finish()
      return
    }
    // Backing out of the first pick leaves the widget off the home screen.
    setResult(RESULT_CANCELED, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
    val dir = Reminders.dataDir(this)
    Thread {
      val json = dir?.let {
        try {
          Native.widgetSources(it)
        } catch (_: Throwable) {
          null
        }
      }
      runOnUiThread { if (!isFinishing) choose(parse(json)) }
    }.start()
  }

  private fun parse(json: String?): List<WidgetSource> {
    val fallback = listOf(
      WidgetSource.TODAY,
      WidgetSource("inbox", "", "Inbox", "view=inbox", """{"project_id":null}"""),
      WidgetSource("upcoming", "", "Upcoming", "view=upcoming", """{"due":"tomorrow"}"""),
    )
    val arr = try {
      JSONArray(json ?: return fallback)
    } catch (_: Exception) {
      return fallback
    }
    val out = (0 until arr.length()).map { WidgetSource.fromRust(arr.getJSONObject(it)) }
    return out.ifEmpty { fallback }
  }

  private fun choose(sources: List<WidgetSource>) {
    val current = HomeWidgets.source(this, widgetId)
    val labels = sources.map {
      when (it.kind) {
        "project" -> "#  ${it.name}"
        "category" -> "        /  ${it.name}"
        else -> it.name
      }
    }.toTypedArray()
    val checked = sources.indexOfFirst { it.kind == current.kind && it.id == current.id }
    val night = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES
    val theme = if (night) android.R.style.Theme_DeviceDefault_Dialog_Alert else android.R.style.Theme_DeviceDefault_Light_Dialog_Alert
    AlertDialog.Builder(this, theme)
      .setTitle("Show in the widget")
      .setSingleChoiceItems(labels, checked) { dialog, i ->
        dialog.dismiss()
        pick(sources[i])
      }
      .setOnCancelListener { finish() }
      .show()
  }

  private fun pick(s: WidgetSource) {
    HomeWidgets.saveSource(this, widgetId, s)
    val mgr = AppWidgetManager.getInstance(this)
    HomeWidgets.update(this, mgr, widgetId)
    mgr.notifyAppWidgetViewDataChanged(intArrayOf(widgetId), R.id.tasks_widget_list)
    HomeWidgets.scheduleMidnight(this)
    setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
    finish()
  }
}

/**
 * A see-through step between a tap and the app, in its own task:
 * - a widget row: opens the task, or ticks it without opening the app;
 * - an app icon shortcut: brings the app up as it is. The launcher starts a
 *   shortcut with CLEAR_TASK, which would start the app's page over.
 */
class TapActivity : Activity() {
  override fun onCreate(saved: Bundle?) {
    super.onCreate(saved)
    val task = intent.getStringExtra(Reminders.EXTRA_TASK)
    when (intent.getStringExtra(HomeWidgets.EXTRA_TAP)) {
      "complete" -> if (task != null) {
        sendBroadcast(Intent(this, TasksWidget::class.java).setAction(HomeWidgets.ACTION_COMPLETE).putExtra(Reminders.EXTRA_TASK, task))
      }
      else -> startActivity(
        HomeWidgets.mainIntent(
          this,
          view = intent.getStringExtra(HomeWidgets.EXTRA_VIEW),
          quickAdd = intent.getStringExtra(HomeWidgets.EXTRA_QUICK_ADD),
          task = task,
        ),
      )
    }
    finish()
    @Suppress("DEPRECATION")
    overridePendingTransition(0, 0)
  }
}

/** The app icon's long-press menu: Add task, Search, Today, Inbox. */
object AppShortcuts {
  fun publish(c: Context) {
    try {
      val shortcuts = QuickAction.values().mapIndexed { rank, a ->
        val intent = Intent(c, TapActivity::class.java).setAction(Intent.ACTION_VIEW).apply {
          a.view?.let { putExtra(HomeWidgets.EXTRA_VIEW, it) }
          a.quickAdd?.let { putExtra(HomeWidgets.EXTRA_QUICK_ADD, it) }
        }
        ShortcutInfoCompat.Builder(c, "tasks-${a.name.lowercase()}")
          .setShortLabel(a.label)
          .setLongLabel(a.longLabel)
          .setIcon(IconCompat.createWithResource(c, a.icon))
          .setIntent(intent)
          .setRank(rank)
          .build()
      }
      ShortcutManagerCompat.setDynamicShortcuts(c, shortcuts)
    } catch (_: Throwable) {
      // A launcher without shortcuts.
    }
  }
}
