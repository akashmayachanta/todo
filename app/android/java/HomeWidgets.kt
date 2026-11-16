// Copied into gen/android by scripts/android-native.mjs. Edit it here, not there.
//
// The home screen widgets and the app icon's shortcuts:
// - TasksWidget lists one list's open tasks: Today unless another was picked
//   (TasksWidgetConfig: Inbox, Upcoming, a project or a section). The lists come
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
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.view.View
import android.widget.LinearLayout
import android.widget.RemoteViews
import android.widget.RemoteViewsService
import android.widget.SeekBar
import android.widget.TextView
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

/**
 * A widget's look, like a desktop pin's: a sticky-note colour (null: the app's own
 * white or black, following the phone's dark mode) and how opaque its background is.
 */
data class WidgetStyle(val color: String?, val opacity: Int) {
  /** The note colour, or null for the app's own. */
  val tint: Int? get() = PALETTE.firstOrNull { it.first == color }?.second

  companion object {
    const val DEFAULT_OPACITY = 96

    /** The desktop pins' pastels (PIN_COLORS in app/src/lib/components/tasks/pins.ts). */
    val PALETTE = listOf(
      "yellow" to 0xFFFDF3B4.toInt(),
      "peach" to 0xFFFDE0C8.toInt(),
      "pink" to 0xFFFBD8E4.toInt(),
      "lavender" to 0xFFE6DCF8.toInt(),
      "blue" to 0xFFD6E8FB.toInt(),
      "mint" to 0xFFD4F1E0.toInt(),
      "lime" to 0xFFE8F2C8.toInt(),
      "sand" to 0xFFECE6DC.toInt(),
    )
  }
}

/** Dark ink for a coloured widget, in either mode: the light theme's colours. */
object WidgetInk {
  const val TEXT = 0xFF1F2937.toInt()
  const val MUTED = 0xFF4B5563.toInt()
  const val DUE = 0xFF15803D.toInt()
  const val OVERDUE = 0xFFDC2626.toInt()
  const val P1 = 0xFFDC2626.toInt()
  const val P2 = 0xFFEA580C.toInt()
  const val P3 = 0xFF2563EB.toInt()
  const val P4 = 0xFF6B7280.toInt()
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

  fun style(c: Context, widgetId: Int): WidgetStyle {
    val p = prefs(c)
    return WidgetStyle(p.getString("c$widgetId", null), p.getInt("o$widgetId", WidgetStyle.DEFAULT_OPACITY).coerceIn(0, 100))
  }

  fun saveStyle(c: Context, widgetId: Int, s: WidgetStyle) {
    prefs(c).edit().putString("c$widgetId", s.color).putInt("o$widgetId", s.opacity).apply()
  }

  fun forget(c: Context, widgetIds: IntArray) {
    val edit = prefs(c).edit()
    for (id in widgetIds) edit.remove("w$id").remove("c$id").remove("o$id")
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
    // The app's own colours come from the layout, so they follow dark mode as it changes.
    val style = style(c, widgetId)
    views.setInt(R.id.tasks_widget_bg, "setImageAlpha", style.opacity * 255 / 100)
    style.tint?.let {
      views.setInt(R.id.tasks_widget_bg, "setColorFilter", it)
      views.setTextColor(R.id.tasks_widget_title, WidgetInk.TEXT)
      views.setTextColor(R.id.tasks_widget_empty, WidgetInk.MUTED)
      views.setInt(R.id.tasks_widget_pick, "setColorFilter", WidgetInk.MUTED)
    }
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
  /** Dark ink on a coloured widget; false keeps the app's own colours. */
  private var inked = false

  override fun onCreate() {}

  override fun onDestroy() {}

  override fun onDataSetChanged() {
    inked = HomeWidgets.style(c, widgetId).tint != null
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
      // A renamed (or deleted) project or section: the title follows.
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
    if (inked) views.setTextColor(R.id.tasks_widget_item_title, WidgetInk.TEXT)
    if (row.due.isEmpty()) {
      views.setViewVisibility(R.id.tasks_widget_item_due, View.GONE)
    } else {
      views.setViewVisibility(R.id.tasks_widget_item_due, View.VISIBLE)
      views.setTextViewText(R.id.tasks_widget_item_due, row.due)
      val due = when {
        inked -> if (row.overdue) WidgetInk.OVERDUE else WidgetInk.DUE
        else -> c.getColor(if (row.overdue) R.color.tasks_widget_overdue else R.color.tasks_widget_due)
      }
      views.setTextColor(R.id.tasks_widget_item_due, due)
    }
    val ring = if (inked) {
      when (row.priority) {
        1 -> WidgetInk.P1
        2 -> WidgetInk.P2
        3 -> WidgetInk.P3
        else -> WidgetInk.P4
      }
    } else {
      c.getColor(
        when (row.priority) {
          1 -> R.color.tasks_widget_p1
          2 -> R.color.tasks_widget_p2
          3 -> R.color.tasks_widget_p3
          else -> R.color.tasks_widget_p4
        },
      )
    }
    views.setInt(R.id.tasks_widget_check, "setColorFilter", ring)
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
        "section" -> "        /  ${it.name}"
        else -> it.name
      }
    }.toTypedArray()
    val checked = sources.indexOfFirst { it.kind == current.kind && it.id == current.id }
    AlertDialog.Builder(this, theme())
      .setTitle("Show in the widget")
      .setSingleChoiceItems(labels, checked) { dialog, i ->
        dialog.dismiss()
        pick(sources[i])
      }
      // A button press dismisses rather than cancels, so this doesn't finish().
      .setNeutralButton("Colour & opacity") { _, _ -> chooseStyle() }
      .setOnCancelListener { finish() }
      .show()
  }

  private val night get() = (resources.configuration.uiMode and Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES

  private fun theme() = if (night) android.R.style.Theme_DeviceDefault_Dialog_Alert else android.R.style.Theme_DeviceDefault_Light_Dialog_Alert

  private fun dp(v: Int) = (v * resources.displayMetrics.density).toInt()

  /** The widget's colour and opacity, like a desktop pin's. Each change shows on the widget at once. */
  private fun chooseStyle() {
    var style = HomeWidgets.style(this, widgetId)
    val ink = if (night) 0xFFF3F4F6.toInt() else WidgetInk.TEXT
    fun save(s: WidgetStyle) {
      style = s
      HomeWidgets.saveStyle(this, widgetId, s)
      redraw()
    }

    val root = LinearLayout(this).apply {
      orientation = LinearLayout.VERTICAL
      setPadding(dp(24), dp(12), dp(24), dp(4))
    }
    fun label(text: String) = TextView(this).apply {
      this.text = text
      setTextColor(ink)
      textSize = 14f
      setPadding(0, dp(12), 0, dp(8))
    }

    root.addView(label("Colour"))
    // The app's own white or black first, then the pastels: two rows of five.
    val keys = listOf<String?>(null) + WidgetStyle.PALETTE.map { it.first }
    val swatches = keys.map { key ->
      Swatch(this, WidgetStyle.PALETTE.firstOrNull { it.first == key }?.second, ink).apply {
        contentDescription = key?.replaceFirstChar { it.uppercase() } ?: "The app's own white or black"
        picked = key == style.color
      }
    }
    swatches.forEachIndexed { i, sw ->
      sw.setOnClickListener {
        swatches.forEach { it.picked = it === sw }
        save(style.copy(color = keys[i]))
      }
    }
    swatches.chunked(5).forEach { row ->
      val line = LinearLayout(this).apply { orientation = LinearLayout.HORIZONTAL }
      row.forEach { line.addView(it, LinearLayout.LayoutParams(dp(40), dp(40)).apply { marginEnd = dp(8); bottomMargin = dp(8) }) }
      root.addView(line)
    }

    val opacityLabel = label("Opacity  ${style.opacity}%")
    root.addView(opacityLabel)
    root.addView(SeekBar(this).apply {
      max = 100
      progress = style.opacity
      setOnSeekBarChangeListener(object : SeekBar.OnSeekBarChangeListener {
        override fun onProgressChanged(bar: SeekBar, value: Int, fromUser: Boolean) {
          opacityLabel.text = "Opacity  $value%"
        }
        override fun onStartTrackingTouch(bar: SeekBar) {}
        // Once on letting go: redrawing the widget on every step would stutter.
        override fun onStopTrackingTouch(bar: SeekBar) = save(style.copy(opacity = bar.progress))
      })
    })

    AlertDialog.Builder(this, theme())
      .setTitle("Widget look")
      .setView(root)
      .setPositiveButton("Done") { _, _ -> done() }
      .setNegativeButton("Reset") { _, _ ->
        save(WidgetStyle(null, WidgetStyle.DEFAULT_OPACITY))
        done()
      }
      .setOnCancelListener { done() }
      .show()
  }

  private fun redraw() {
    val mgr = AppWidgetManager.getInstance(this)
    HomeWidgets.update(this, mgr, widgetId)
    mgr.notifyAppWidgetViewDataChanged(intArrayOf(widgetId), R.id.tasks_widget_list)
  }

  /** The widget keeps its list (Today when new). */
  private fun done() {
    HomeWidgets.scheduleMidnight(this)
    setResult(RESULT_OK, Intent().putExtra(AppWidgetManager.EXTRA_APPWIDGET_ID, widgetId))
    finish()
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

/** A colour dot in the widget's look dialog; no colour is the app's own, half white and half black. */
class Swatch(c: Context, private val color: Int?, private val ring: Int) : View(c) {
  var picked = false
    set(v) {
      field = v
      invalidate()
    }
  private val fill = Paint(Paint.ANTI_ALIAS_FLAG)
  private val edge = Paint(Paint.ANTI_ALIAS_FLAG).apply { style = Paint.Style.STROKE }
  private val box = RectF()

  init {
    isClickable = true
    isFocusable = true
  }

  override fun onDraw(canvas: Canvas) {
    val d = resources.displayMetrics.density
    val inset = 5 * d
    box.set(inset, inset, width - inset, height - inset)
    if (color != null) {
      fill.color = color
      canvas.drawOval(box, fill)
    } else {
      fill.color = 0xFFFFFFFF.toInt()
      canvas.drawArc(box, 135f, 180f, true, fill)
      fill.color = 0xFF1F2023.toInt()
      canvas.drawArc(box, -45f, 180f, true, fill)
    }
    edge.color = 0x33000000
    edge.strokeWidth = d
    canvas.drawOval(box, edge)
    if (picked) {
      edge.color = ring
      edge.strokeWidth = 2 * d
      box.inset(-3 * d, -3 * d)
      canvas.drawOval(box, edge)
    }
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
