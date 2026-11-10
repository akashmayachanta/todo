<script lang="ts">
	import { onMount, tick } from 'svelte';
	import { page } from '$app/state';
	import { goto } from '$app/navigation';
	import { resolve } from '$app/paths';
	import { api, errorDetail } from '$lib/api';
	import type { Task, TaskCategory, TaskFilter, TaskFilterIn, TaskGroupBy, TaskIn, TaskPatch, TaskPriority, TaskProject } from '$lib/api';
	import { toast } from '$lib/stores/toast';
	import { confirmDialog } from '$lib/stores/confirm';
	import { askReminderPermission } from '$lib/stores/taskReminders';
	import { onTaskAdded, setQuickAddContext } from '$lib/stores/taskQuickAdd';
	import { taskSettings } from '$lib/stores/taskSettings';
	import { onTasksChanged } from '$lib/tasks/client';
	import { plainText } from '$lib/links';
	import { sorting, type SortApi, type SortDrop, type SortKind } from '$lib/sortable';
	import { addDays, dayLabel, daysBetween, formatDuration, fromYmd, todayHeading, ymd } from '$lib/taskParse';
	import DropdownMenu from '$lib/components/DropdownMenu.svelte';
	import TasksPane from '$lib/components/tasks/TasksPane.svelte';
	import QuickAdd from '$lib/components/tasks/QuickAdd.svelte';
	import TaskRow from '$lib/components/tasks/TaskRow.svelte';
	import TaskSearchBox from '$lib/components/tasks/TaskSearchBox.svelte';
	import TodayIcon from '$lib/components/tasks/TodayIcon.svelte';
	import TaskDetail from '$lib/components/tasks/TaskDetail.svelte';
	import TaskInlineEdit from '$lib/components/tasks/TaskInlineEdit.svelte';
	import TaskCalendar from '$lib/components/tasks/TaskCalendar.svelte';
	import TaskKarma from '$lib/components/tasks/TaskKarma.svelte';
	import LayoutToggle from '$lib/components/tasks/LayoutToggle.svelte';
	import SmartTips from '$lib/components/tasks/SmartTips.svelte';
	import GroupMenu from '$lib/components/tasks/GroupMenu.svelte';
	import FilterDialog from '$lib/components/tasks/FilterDialog.svelte';
	import {
		DATE_VIEW_GROUPS, FILTER_GROUPS, compileFilter, filterDefaults, groupTasks, loadGrouping, saveGrouping,
		type AddDefaults, type Bucket, type DateViewGroup, type GroupCtx,
	} from '$lib/components/tasks/taskFilters';
	import {
		PROJECT_COLORS, byDue, byPosition, completionPatch, durationTotals, loadLayouts, parentFinder, placeName, projectColor, saveLayouts, shownWhenDue, parentMinutes, subtaskMinutes, taskRoots,
		shiftInstant, type TaskLayout, type TaskSettings, type TaskView,
	} from '$lib/components/tasks/shared';
	import type { PinTarget } from '$lib/components/tasks/pins';
	import { setTaskbarCount } from '$shell/taskbar';
	import { isDesktop, pinToDesktop } from '$shell/widgets';
	import './tasks.css';

	const UPCOMING_DAYS = 14;

	/** The phone tab bar. */
	const TABS = [
		{ key: 'inbox',    label: 'Inbox',    icon: 'M20 13V6a2 2 0 00-2-2H6a2 2 0 00-2 2v7m16 0v5a2 2 0 01-2 2H6a2 2 0 01-2-2v-5m16 0h-2.586a1 1 0 00-.707.293l-2.414 2.414a1 1 0 01-.707.293h-3.172a1 1 0 01-.707-.293l-2.414-2.414A1 1 0 006.586 13H4' },
		{ key: 'today',    label: 'Today' }, // TodayIcon: a calendar tile with the day number
		{ key: 'upcoming', label: 'Upcoming', icon: 'M8 7V3m8 4V3m-9 8h10M5 21h14a2 2 0 002-2V7a2 2 0 00-2-2H5a2 2 0 00-2 2v12a2 2 0 002 2z' },
		{ key: 'karma',    label: 'Karma',    icon: 'M4 20h16M7 16v-4m5 4V8m5 8v-7' },
		{ key: 'browse',   label: 'Browse',   icon: 'M4 6h16M4 12h16M4 18h10' },
	] as const;

	let projects = $state<TaskProject[]>([]);
	let categories = $state<TaskCategory[]>([]);
	let filters = $state<TaskFilter[]>([]);
	/** Open tasks, each open one with all its subtasks (done ones too). */
	let tasks = $state<Task[]>([]);
	let loading = $state(true);
	let error = $state('');
	let now = $state(new Date());

	/** Which "+ Add task" has its quick add open: 'main', `cat:<id>` or `date:<YYYY-MM-DD>`. */
	let addingAt = $state<string | null>(null);

	let completed = $state<Task[]>([]);
	/** The task in the drawer when it isn't among the open ones (done, or opened from search). */
	let extraTask = $state<Task | null>(null);

	/** The layout picked for each view, remembered in this browser. */
	let layouts = $state<Record<string, TaskLayout>>({});
	/** How Today and Upcoming are grouped, remembered in this browser (a filter keeps its own). */
	let grouping = $state<Record<string, DateViewGroup>>({});
	/** Display settings (Settings → General), remembered in this browser. */
	let settings = $derived($taskSettings);

	function changeSettings(next: TaskSettings) {
		taskSettings.set(next);
	}
	/** The task being edited where it stands (Alt+click or the pencil). */
	let inlineEditId = $state<string | null>(null);

	/** The task being dragged across the board, and the column it is over. */
	let dragging = $state<Task | null>(null);
	let dropOver = $state<string | null>(null);

	let projectDialog = $state<{ id: string | null; name: string; color: string } | null>(null);
	/** The filter being edited, 'new' for a new one. */
	let filterDialog = $state<TaskFilter | 'new' | null>(null);
	let addingCategory = $state(false);
	let newCategory = $state('');
	let renamingCategory = $state<string | null>(null);
	let categoryName = $state('');
	/** The category being moved to another project. */
	let movingCategory = $state<TaskCategory | null>(null);
	let pane = $state<TasksPane | null>(null);

	// ── Where we are (kept in the URL) ─────────────────────────────────────────

	let params = $derived(page.url.searchParams);
	let projectId = $derived(params.get('project') || null);
	let filterId = $derived(projectId ? null : params.get('filter') || null);
	let searchQuery = $derived(projectId || filterId ? '' : params.get('q') ?? '');
	let view = $derived.by((): TaskView => {
		if (projectId) return 'project';
		if (filterId) return 'filter';
		if (searchQuery) return 'search';
		const v = params.get('view');
		// With no view in the URL, the one picked in Settings ("Open on").
		return v === 'inbox' || v === 'upcoming' || v === 'today' || v === 'karma' ? v : settings.startView;
	});
	let taskId = $derived(params.get('task') || null);
	/** Done tasks are listed in the Inbox and projects when Settings → General says so. */
	let completedShown = $derived(settings.showCompleted && (view === 'project' || view === 'inbox'));
	/** Phones: the Browse tab (account, filters, projects) in place of the list. */
	let browsing = $derived(!projectId && !filterId && params.get('view') === 'browse');
	let project = $derived(projects.find((p) => p.id === projectId) ?? null);
	let filter = $derived(filters.find((f) => f.id === filterId) ?? null);
	let today = $derived(ymd(now));
	let tomorrow = $derived(addDays(today, 1));
	let groupCtx = $derived<GroupCtx>({ projects, categories, today });

	function go(query: Record<string, string | number | null>, replaceState = false) {
		const q = new URLSearchParams();
		for (const [k, v] of Object.entries(query)) if (v !== null) q.set(k, String(v));
		const s = q.toString();
		goto(`${resolve('/tasks' as any)}${s ? `?${s}` : ''}`, { noScroll: true, keepFocus: true, replaceState });
	}

	function here(): Record<string, string | number | null> {
		if (view === 'search') return { q: searchQuery };
		return projectId ? { project: projectId } : filterId ? { filter: filterId } : { view };
	}

	function openTask(task: Task) {
		go({ ...here(), task: task.id });
	}

	function closeTask() {
		go(here());
	}

	// ── What is shown ──────────────────────────────────────────────────────────

	let subtaskMap = $derived.by(() => {
		const map = new Map<string, Task[]>();
		for (const t of tasks) {
			if (t.parent_id === null) continue;
			const list = map.get(t.parent_id) ?? [];
			list.push(t);
			map.set(t.parent_id, list);
		}
		for (const list of map.values()) list.sort(byPosition);
		return map;
	});
	const subtasksOf = (id: string) => subtaskMap.get(id) ?? [];
	let parentOf = $derived(parentFinder(tasks));

	let open = $derived(tasks.filter((t) => !t.completed_at));
	let topLevel = $derived(open.filter((t) => t.parent_id === null));
	let dated = $derived(open.filter((t) => t.due_date));
	let lastUpcoming = $derived(addDays(today, UPCOMING_DAYS));

	let counts = $derived.by(() => {
		const byProject: Record<string, number> = {};
		const byCategory: Record<string, number> = {};
		for (const t of topLevel) {
			if (t.project_id !== null) byProject[t.project_id] = (byProject[t.project_id] ?? 0) + 1;
			if (t.category_id !== null) byCategory[t.category_id] = (byCategory[t.category_id] ?? 0) + 1;
		}
		const byFilter: Record<string, number> = {};
		for (const f of filters) byFilter[f.id] = open.filter(compileFilter(f.criteria, groupCtx)).length;
		return {
			filters: byFilter,
			inbox: topLevel.filter((t) => t.project_id === null).length,
			today: dated.filter((t) => t.due_date! <= today).length,
			upcoming: dated.filter((t) => t.due_date! > today && t.due_date! <= lastUpcoming).length,
			projects: byProject,
			categories: byCategory,
		};
	});
	/** Minutes of open work rolled up to each project and category (subtasks included). */
	let minutes = $derived(durationTotals(open, subtasksOf));

	let inboxTasks = $derived(topLevel.filter((t) => t.project_id === null).sort(byPosition));
	let overdue = $derived(dated.filter((t) => t.due_date! < today).sort(byDue));
	let todayTasks = $derived(dated.filter((t) => t.due_date === today).sort(byDue));
	/**
	 * What Today lists: due subtasks under their parents, once each, so a parent with
	 * anything overdue sits in Overdue. With subtasks turned off, every due task on its own.
	 */
	let overdueRows = $derived(settings.showSubtasks ? taskRoots(overdue, parentOf) : overdue);
	let todayRows = $derived.by(() => {
		if (!settings.showSubtasks) return todayTasks;
		const inOverdue = new Set(overdueRows.map((t) => t.id));
		return taskRoots(todayTasks, parentOf).filter((t) => !inOverdue.has(t.id));
	});
	let dueSubtasks = $derived(shownWhenDue(today));
	let upcomingDays = $derived.by(() => {
		const days: { date: string; tasks: Task[] }[] = [];
		// Upcoming starts tomorrow: today's tasks are in Today.
		for (let i = 1; i <= UPCOMING_DAYS; i++) {
			const date = addDays(today, i);
			const list = dated.filter((t) => t.due_date === date).sort(byDue);
			if (list.length) days.push({ date, tasks: list });
		}
		return days;
	});
	let later = $derived(dated.filter((t) => t.due_date! > lastUpcoming).sort(byDue));

	/** A filter's open tasks (subtasks too, flat, like Upcoming), the details its rows show, and what a task added there gets. */
	let filterTest = $derived(filter ? compileFilter(filter.criteria, groupCtx) : null);
	let filterTasks = $derived(filterTest ? open.filter(filterTest).sort(byDue) : []);
	let filterFields = $derived(filter ? new Set(filter.fields) : null);
	let filterBase = $derived<AddDefaults>(filter ? filterDefaults(filter.criteria, groupCtx) : {});

	let projectCategories = $derived(categories.filter((c) => c.project_id === projectId).sort((a, b) => a.position - b.position || a.id.localeCompare(b.id)));
	let uncategorised = $derived(topLevel.filter((t) => t.project_id === projectId && t.category_id === null).sort(byPosition));
	const inCategory = (id: string) => topLevel.filter((t) => t.category_id === id).sort(byPosition);

	let drawerTask = $derived(
		taskId === null ? null
			: tasks.find((t) => t.id === taskId) ?? completed.find((t) => t.id === taskId) ?? (extraTask?.id === taskId ? extraTask : null),
	);
	let drawerParent = $derived(drawerTask?.parent_id ? tasks.find((t) => t.id === drawerTask!.parent_id) ?? null : null);

	/** "6 Nov" over "Tomorrow · Friday" or "Saturday". */
	function dayHeading(date: string) {
		const d = fromYmd(date);
		const weekday = d.toLocaleDateString(undefined, { weekday: 'long' });
		return {
			title: d.toLocaleDateString(undefined, { day: 'numeric', month: 'short' }),
			sub: daysBetween(today, date) === 1 ? `Tomorrow · ${weekday}` : weekday,
		};
	}

	function placeOf(task: Task) {
		const p = projects.find((x) => x.id === task.project_id);
		return { name: placeName(task, projects, categories), color: p ? projectColor(p.color) : null };
	}

	// ── Layout: list, board or calendar ────────────────────────────────────────

	let layoutKey = $derived(view === 'project' ? `project:${projectId}` : view === 'filter' ? `filter:${filterId}` : view);
	/** Today has no calendar: it is one day (and the overdue ones). */
	let layoutOptions = $derived<readonly TaskLayout[]>(view === 'search' ? ['list'] : view === 'today' ? ['list', 'board'] : ['list', 'board', 'calendar']);
	let layout = $derived.by((): TaskLayout => {
		const l = layouts[layoutKey];
		return l && layoutOptions.includes(l) ? l : 'list';
	});

	function setLayout(l: TaskLayout) {
		layouts = { ...layouts, [layoutKey]: l };
		saveLayouts(layouts);
		addingAt = null;
		addingCategory = false;
	}

	// ── Grouping: Today, Upcoming and filters ──────────────────────────────────

	/** 'date' is Today's and Upcoming's own day sections; 'none' a filter's flat list. */
	let groupBy = $derived.by((): TaskGroupBy | 'date' => {
		if (view === 'filter') return filter?.group_by ?? 'none';
		if (view === 'today' || view === 'upcoming') return grouping[view] ?? 'date';
		return 'date';
	});

	/** What a task added in this view starts with, before its bucket's own values. */
	let baseDefaults = $derived<AddDefaults>(view === 'filter' ? filterBase : { due_date: view === 'upcoming' ? tomorrow : today });

	/** The view's tasks in buckets, or null when it isn't grouped (or is a calendar). */
	let buckets = $derived.by((): Bucket[] | null => {
		if (groupBy === 'date' || groupBy === 'none' || layout === 'calendar') return null;
		const source = view === 'filter' ? filterTasks
			: view === 'today' ? [...overdueRows, ...todayRows]
			: view === 'upcoming' ? dated.filter((t) => t.due_date! > today)
			: null;
		if (!source) return null;
		return groupTasks(source, groupBy, groupCtx, { allPriorities: layout === 'board' })
			.map((b) => ({ ...b, defaults: { ...baseDefaults, ...b.defaults } }));
	});

	let groupOptions = $derived(view === 'filter' ? FILTER_GROUPS : DATE_VIEW_GROUPS);
	let groupShown = $derived(layout !== 'calendar' && (view === 'today' || view === 'upcoming' || (view === 'filter' && !!filter)));

	async function setGroup(value: string) {
		addingAt = null;
		if (view === 'today' || view === 'upcoming') {
			grouping = { ...grouping, [view]: value as DateViewGroup };
			saveGrouping(grouping);
			return;
		}
		if (view !== 'filter' || !filter) return;
		const before = filter;
		// Shown at once; put back if the server says no.
		filters = filters.map((f) => (f.id === before.id ? { ...f, group_by: value as TaskGroupBy } : f));
		try {
			const saved = await api.tasks.filters.update(before.id, {
				name: before.name, color: before.color, criteria: before.criteria, fields: before.fields, group_by: value as TaskGroupBy,
			});
			filters = filters.map((f) => (f.id === saved.id ? saved : f));
		} catch (e) {
			filters = filters.map((f) => (f.id === before.id ? before : f));
			toast.error(errorDetail(e));
		}
	}

	type Column = {
		key: string;
		title: string;
		sub?: string;
		/** A dot before the title (a project's colour). */
		color?: string | null;
		tasks: Task[];
		overdue?: boolean;
		category?: TaskCategory;
		/** What dropping a task here changes; null when nothing can be dropped here. */
		drop: TaskPatch | null;
		add: { key: string; defaults: AddDefaults } | null;
	};

	/** The board's columns: priorities in the Inbox, days in Today and Upcoming, categories in a project. */
	let columns = $derived.by((): Column[] => {
		if (buckets) {
			return buckets.map((b) => ({
				key: b.key, title: b.title, sub: b.sub, color: b.color, overdue: b.overdue, tasks: b.tasks,
				drop: b.drop, add: b.drop ? { key: b.key, defaults: b.defaults } : null,
			}));
		}
		if (view === 'filter') {
			return filter ? [{ key: 'filter', title: 'Tasks', tasks: filterTasks, drop: null, add: { key: 'main', defaults: filterBase } }] : [];
		}
		if (view === 'inbox') {
			return ([1, 2, 3, 4] as TaskPriority[]).map((p) => ({
				key: `pri:${p}`,
				title: `Priority ${p}`,
				tasks: inboxTasks.filter((t) => t.priority === p),
				drop: { priority: p },
				add: { key: p === 4 ? 'main' : `pri:${p}`, defaults: { priority: p } },
			}));
		}
		if (view === 'today') {
			const cols: Column[] = [];
			if (overdue.length) cols.push({ key: 'overdue', title: 'Overdue', tasks: overdueRows, overdue: true, drop: null, add: null });
			cols.push({ key: `date:${today}`, title: todayHeading(today), tasks: todayRows, drop: { due_date: today }, add: { key: 'main', defaults: { due_date: today } } });
			return cols;
		}
		if (view === 'upcoming') {
			const cols: Column[] = [];
			for (let i = 1; i <= UPCOMING_DAYS; i++) {
				const date = addDays(today, i);
				const head = dayHeading(date);
				cols.push({
					key: `date:${date}`,
					title: head.title,
					sub: head.sub,
					tasks: dated.filter((t) => t.due_date === date).sort(byDue),
					drop: { due_date: date },
					add: { key: `date:${date}`, defaults: { due_date: date } },
				});
			}
			if (later.length) cols.push({ key: 'later', title: 'Later', tasks: later, drop: null, add: null });
			return cols;
		}
		if (!project) return [];
		// "No category" only when it has tasks, or is the project's only column, or its quick add is open (q).
		const showNone = uncategorised.length > 0 || projectCategories.length === 0 || addingAt === 'main';
		return [
			...(showNone
				? [{ key: 'cat:none', title: 'No category', tasks: uncategorised, drop: { category_id: null }, add: { key: 'main', defaults: { project_id: project.id } } }]
				: []),
			...projectCategories.map((c) => ({
				key: `cat:${c.id}`,
				title: c.name,
				category: c,
				tasks: inCategory(c.id),
				drop: { category_id: c.id },
				add: { key: `cat:${c.id}`, defaults: { project_id: project!.id, category_id: c.id } },
			})),
		];
	});

	/** What the calendar shows: every dated task in Upcoming, the view's own tasks elsewhere. */
	let calendarTasks = $derived(
		view === 'upcoming' ? dated
			: view === 'filter' ? filterTasks.filter((t) => t.due_date)
			: topLevel.filter((t) => t.due_date && (view === 'inbox' ? t.project_id === null : t.project_id === projectId)),
	);
	let calendarUndated = $derived(
		view === 'upcoming' ? null
			: view === 'filter' ? filterTasks.filter((t) => !t.due_date)
			: topLevel.filter((t) => !t.due_date && (view === 'inbox' ? t.project_id === null : t.project_id === projectId)),
	);
	/** Where a task added from the calendar goes. */
	let calendarDefaults = $derived<AddDefaults>(view === 'project' && projectId ? { project_id: projectId } : view === 'filter' ? filterBase : {});

	function moveToDate(task: Task, date: string | null) {
		patchTask(task, date ? { due_date: date } : { due_date: null, due_time: null });
	}

	// ── Board drag and drop ────────────────────────────────────────────────────

	function cardDragStart(e: DragEvent, task: Task) {
		// Its grip is being dragged, to reorder within the column.
		if (sorting()) {
			e.preventDefault();
			return;
		}
		dragging = task;
		e.dataTransfer?.setData('text/plain', String(task.id));
		if (e.dataTransfer) e.dataTransfer.effectAllowed = 'move';
	}

	function cardDragEnd() {
		dragging = null;
		dropOver = null;
	}

	/** A subtask follows its parent, so it can't be dropped on a project or category. */
	function canDrop(task: Task, col: Column): boolean {
		if (!col.drop) return false;
		return task.parent_id === null || !('project_id' in col.drop || 'category_id' in col.drop);
	}

	function colDragOver(e: DragEvent, col: Column) {
		if (!dragging || !canDrop(dragging, col)) return;
		e.preventDefault();
		if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
		dropOver = col.key;
	}

	function colDragLeave(e: DragEvent, col: Column) {
		const el = e.currentTarget as HTMLElement;
		if (dropOver === col.key && !el.contains(e.relatedTarget as Node | null)) dropOver = null;
	}

	function colDrop(e: DragEvent, col: Column) {
		e.preventDefault();
		const task = dragging;
		cardDragEnd();
		if (!task || !col.drop || !canDrop(task, col)) return;
		// Only send what actually changes; dropping back on its own column does nothing.
		const patch = Object.fromEntries(
			Object.entries(col.drop).filter(([k, v]) => task[k as keyof Task] !== v),
		) as TaskPatch;
		if (Object.keys(patch).length) patchTask(task, patch);
	}

	// ── Rearranging by the grip ────────────────────────────────────────────────

	const topSiblings = (projectId: string | null, categoryId: string | null) =>
		topLevel.filter((t) => t.project_id === projectId && t.category_id === categoryId).sort(byPosition);

	/**
	 * Puts a task where it was dropped. A task dropped in another category joins it, and
	 * a subtask dropped under another task moves to it. Shown at once, put back on failure.
	 */
	async function dropTask(d: SortDrop) {
		const task = tasks.find((t) => t.id === d.id);
		const target = d.targetId ? tasks.find((t) => t.id === d.targetId) ?? null : null;
		if (!task) return;

		let moved: Task;
		let list: Task[];
		let patch: TaskPatch | null = null;
		if (d.kind === 'sub') {
			const parentId = d.into ? d.targetId : target?.parent_id;
			const parent = tasks.find((t) => t.id === parentId);
			if (!parent) return;
			if (parent.id !== task.parent_id) patch = { parent_id: parent.id };
			moved = { ...task, parent_id: parent.id, project_id: parent.project_id, category_id: parent.category_id };
			list = subtasksOf(parent.id);
		} else {
			const categoryId = d.zone !== null ? (d.zone === 'none' ? null : d.zone) : target?.category_id ?? null;
			if (categoryId !== task.category_id) patch = { category_id: categoryId };
			moved = { ...task, category_id: categoryId };
			list = topSiblings(task.project_id, categoryId);
		}

		const others = list.filter((t) => t.id !== task.id);
		const at = target && !d.into ? others.findIndex((t) => t.id === target.id) + (d.after ? 1 : 0) : others.length;
		const ordered = [...others.slice(0, at), moved, ...others.slice(at)];
		const ids = ordered.map((t) => t.id);
		if (!patch && ids.every((id, i) => id === list[i]?.id)) return;

		const before = tasks;
		const slots = ordered.map((t) => t.position).sort((a, b) => a - b);
		const next = new Map<string, Task>();
		let last = -Infinity;
		ordered.forEach((t, i) => {
			last = Math.max(slots[i], last + 1);
			next.set(t.id, { ...t, position: last });
		});
		tasks = tasks.map((t) => next.get(t.id) ?? t);

		try {
			if (patch) await api.tasks.update(task.id, patch);
			const saved = new Map((await api.tasks.reorder(ids)).map((t) => [t.id, t]));
			tasks = tasks.map((t) => saved.get(t.id) ?? t);
			// A task's subtasks follow it to another category.
			if (patch) await reloadTasks();
			// A subtask with a duration takes it from one task's total to the other's.
			if (patch?.parent_id && task.duration_minutes) {
				await syncParentMinutes(patch.parent_id);
				await syncParentMinutes(task.parent_id, true);
			}
		} catch (e) {
			tasks = before;
			toast.error(errorDetail(e));
			await reloadTasks();
		}
	}

	/** The arrow keys on a grip: a place up or down among the tasks shown with it. */
	function stepTask(id: string, kind: SortKind, by: -1 | 1) {
		const task = tasks.find((t) => t.id === id);
		if (!task) return;
		const list = kind === 'sub'
			? subtasksOf(task.parent_id!).filter((s) => !!s.completed_at === !!task.completed_at)
			: topSiblings(task.project_id, task.category_id);
		const other = list[list.findIndex((t) => t.id === id) + by];
		if (other) dropTask({ id, kind, targetId: other.id, after: by === 1, into: false, zone: null });
	}

	const sortApi: SortApi = { ondrop: dropTask, onstep: stepTask };
	/** Lists in manual order have grips; Today, Upcoming, filters and search sort themselves. */
	let sortable = $derived(view === 'inbox' || view === 'project');

	// ── Stepping through tasks in the modal ────────────────────────────────────

	/** A task and the subtasks listed under it, in the views that nest them. */
	function withSubtasks(list: Task[]): Task[] {
		if (!((view === 'inbox' || view === 'project' || view === 'today') && settings.showSubtasks)) return list;
		return list.flatMap((t) => [
			t,
			...subtasksOf(t.id).filter((s) => (settings.showCompleted || !s.completed_at) && (view !== 'today' || dueSubtasks(t, s))),
		]);
	}

	/** The tasks of the view in the order they are shown, for the modal's up / down. */
	let navOrder = $derived.by((): Task[] => {
		if (view === 'search') return [...searchOpen, ...searchDone];
		let list: Task[];
		if (layout === 'calendar') list = [...calendarTasks].sort(byDue).concat(calendarUndated ?? []);
		else if (layout === 'board' || buckets) list = withSubtasks(columns.flatMap((c) => c.tasks));
		else if (view === 'filter') list = filterTasks;
		else if (view === 'inbox') list = withSubtasks(inboxTasks);
		else if (view === 'today') list = withSubtasks([...overdueRows, ...todayRows]);
		else if (view === 'upcoming') list = [...upcomingDays.flatMap((d) => d.tasks), ...later];
		else list = withSubtasks([...uncategorised, ...projectCategories.flatMap((c) => inCategory(c.id))]);
		return completedShown && layout !== 'calendar' ? [...list, ...completed] : list;
	});

	/** The tasks either side of the open one: in the view, else among its sibling subtasks. */
	let neighbours = $derived.by((): { prev: Task | null; next: Task | null } => {
		const t = drawerTask;
		if (!t) return { prev: null, next: null };
		let list = navOrder;
		let i = list.findIndex((x) => x.id === t.id);
		if (i < 0 && t.parent_id !== null) {
			list = subtasksOf(t.parent_id);
			i = list.findIndex((x) => x.id === t.id);
		}
		if (i < 0) return { prev: null, next: null };
		return { prev: list[i - 1] ?? null, next: list[i + 1] ?? null };
	});

	let heading = $derived(
		view === 'project' ? project?.name ?? 'Project'
			: view === 'filter' ? filter?.name ?? 'Filter'
			: view === 'inbox' ? 'Inbox' : view === 'today' ? 'Today' : view === 'karma' ? 'Karma' : view === 'search' ? 'Search' : 'Upcoming',
	);

	// ── Search ─────────────────────────────────────────────────────────────────

	const SEARCH_OPEN_LIMIT = 200;
	const SEARCH_DONE_LIMIT = 100;

	let searchOpen = $state<Task[]>([]);
	let searchDone = $state<Task[]>([]);
	/** Where to go back to when the search is cleared. */
	let beforeSearch: Record<string, string | number | null> = {};
	/** Bumped on each search, so a slow one already typed past is dropped. */
	let searchLoad = 0;

	async function loadSearch() {
		const q = searchQuery;
		const n = ++searchLoad;
		if (!q) {
			searchOpen = [];
			searchDone = [];
			return;
		}
		const [found, done] = await Promise.all([
			api.tasks.list({ q, limit: SEARCH_OPEN_LIMIT }),
			api.tasks.list({ q, completed: true, limit: SEARCH_DONE_LIMIT }),
		]);
		if (n !== searchLoad) return;
		searchOpen = found;
		searchDone = done;
	}

	$effect(() => {
		void searchQuery;
		loadSearch().catch((e) => toast.error(errorDetail(e)));
	});

	function runSearch(text: string) {
		const q = text.trim();
		if (q) {
			if (view !== 'search') beforeSearch = browsing ? { view: 'browse' } : here();
			go({ q }, true);
		} else if (view === 'search') {
			go(beforeSearch, true);
		}
	}

	// ── Pins ───────────────────────────────────────────────────────────────────

	/** What the header's pin button pins, on the desktop. */
	let pinTarget = $derived.by((): PinTarget | null => {
		if (!isDesktop) return null;
		if (view === 'inbox' || view === 'today' || view === 'upcoming') return { kind: view, id: null };
		if (view === 'project' && project) return { kind: 'project', id: project.id };
		if (view === 'filter' && filter) return { kind: 'filter', id: filter.id };
		return null;
	});

	function pin(target: PinTarget) {
		pinToDesktop(target).catch((e) => toast.error(errorDetail(e)));
	}

	// Today's count on the Windows taskbar, the same number as the pane.
	$effect(() => {
		if (!loading && !error) setTaskbarCount(counts.today);
	});

	// ── Loading ────────────────────────────────────────────────────────────────

	async function loadAll() {
		try {
			[projects, categories, tasks, filters] = await Promise.all([
				api.tasks.projects.list(), api.tasks.categories.list(), api.tasks.list(), api.tasks.filters.list(),
			]);
			error = '';
		} catch (e) {
			error = errorDetail(e);
		} finally {
			loading = false;
		}
	}

	async function reloadTasks() {
		try {
			tasks = await api.tasks.list();
			if (completedShown) await loadCompleted();
			if (view === 'search') await loadSearch();
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	/** Bumped on each load, so a slow load for a view already left is dropped. */
	let completedLoad = 0;

	async function loadCompleted() {
		const n = ++completedLoad;
		const list = view === 'project' && projectId
			? await api.tasks.list({ completed: true, project_id: projectId })
			: await api.tasks.list({ completed: true, inbox: true });
		if (n === completedLoad) completed = list;
	}

	// The done tasks of the view on screen, loaded when it opens or the setting is turned on.
	$effect(() => {
		void view; void projectId;
		completed = [];
		completedLoad++;
		if (!completedShown) return;
		loadCompleted().catch((e) => toast.error(errorDetail(e)));
	});

	// A new view starts with no quick add open.
	$effect(() => {
		void view; void projectId; void filterId;
		addingAt = null;
		addingCategory = false;
		renamingCategory = null;
		inlineEditId = null;
	});

	// A category picked in the pane (#task-cat-<id>) is scrolled into view once it is on screen.
	$effect(() => {
		const hash = page.url.hash;
		void layout; void projectId;
		if (loading || !hash.startsWith('#task-cat-')) return;
		tick().then(() => document.getElementById(hash.slice(1))?.scrollIntoView({ behavior: 'smooth', block: 'start' }));
	});

	// ?task= for a task not in the open list (e.g. a done one, from search) is fetched.
	$effect(() => {
		const id = taskId;
		if (id === null || loading || tasks.some((t) => t.id === id) || completed.some((t) => t.id === id) || extraTask?.id === id) return;
		api.tasks.get(id)
			.then((t) => (extraTask = t))
			.catch(() => {
				toast.error('That task no longer exists');
				closeTask();
			});
	});

	// ── Changes ────────────────────────────────────────────────────────────────

	function replace(updated: Task) {
		tasks = tasks.map((t) => (t.id === updated.id ? updated : t));
		completed = completed.map((t) => (t.id === updated.id ? updated : t));
		searchOpen = searchOpen.map((t) => (t.id === updated.id ? updated : t));
		searchDone = searchDone.map((t) => (t.id === updated.id ? updated : t));
		if (extraTask?.id === updated.id) extraTask = updated;
	}

	function shownHere(task: Task): boolean {
		if (view === 'project') return task.project_id === projectId;
		if (view === 'inbox') return task.project_id === null;
		if (view === 'filter') return filterTest?.(task) ?? false;
		if (view === 'today') return !!task.due_date && task.due_date <= today;
		if (view === 'search') return false;
		return !!task.due_date && task.due_date > today;
	}

	async function addTask(data: TaskIn) {
		try {
			const created = await api.tasks.create(data);
			tasks = [...tasks, created];
			if (created.remind_at) askReminderPermission();
			if (!shownHere(created)) toast.success(`Added to ${placeName(created, projects, categories)}${created.due_date ? `, ${dayLabel(created.due_date, now)}` : ''}`);
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}

	async function addSubtask(parent: Task, data: Omit<TaskIn, 'parent_id' | 'project_id' | 'category_id'>) {
		try {
			const created = await api.tasks.create({ ...data, parent_id: parent.id });
			tasks = [...tasks, created];
			if (created.remind_at) askReminderPermission();
			if (created.duration_minutes) await syncParentMinutes(parent.id);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	/** A task whose subtasks have durations takes their remaining total as its own, saved like any edit. */
	async function syncParentMinutes(parentId: string | null, hadMinutes = false) {
		const parent = parentId ? tasks.find((t) => t.id === parentId) : null;
		if (!parent) return;
		const next = parentMinutes(parent, subtasksOf(parent.id), hadMinutes);
		if (next !== undefined) replace(await api.tasks.update(parent.id, { duration_minutes: next }));
	}

	async function patchTask(task: Task, patch: TaskPatch) {
		// Moving a task to another day (board, calendar) moves its reminder by as many days.
		if (patch.due_date && task.due_date && task.remind_at && !('remind_at' in patch) && patch.due_date !== task.due_date) {
			patch = { ...patch, remind_at: shiftInstant(task.remind_at, daysBetween(task.due_date, patch.due_date)) };
		}
		// A task with timed subtasks takes their total; its own can't be set apart from them.
		if ('duration_minutes' in patch && task.parent_id === null && subtaskMinutes(subtasksOf(task.id)) !== null) {
			const { duration_minutes: _, ...rest } = patch;
			patch = rest;
			if (!Object.keys(patch).length) return;
		}
		try {
			const updated = await api.tasks.update(task.id, patch);
			replace(updated);
			if (patch.remind_at) askReminderPermission();
			// A move carries the subtasks along on the server.
			if (('project_id' in patch || 'category_id' in patch) && task.parent_id === null) await reloadTasks();
			if (task.parent_id !== null && ('duration_minutes' in patch || 'parent_id' in patch)) {
				const had = !!task.duration_minutes;
				const moved = updated.parent_id !== task.parent_id;
				await syncParentMinutes(updated.parent_id, had && !moved);
				if (moved) await syncParentMinutes(task.parent_id, had);
			}
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function toggleTask(task: Task) {
		try {
			const patch = completionPatch(task, now);
			replace(await api.tasks.update(task.id, patch));
			// A done subtask's time leaves its task's remaining total; an undone one comes back.
			if (task.parent_id !== null && task.duration_minutes) await syncParentMinutes(task.parent_id);
			if (patch.completes_occurrence) {
				toast.success(`Next: ${dayLabel(patch.due_date!, now)}`);
				return;
			}
			if (task.completed_at) completed = completed.filter((t) => t.id !== task.id);
			else if (task.parent_id === null) toast.success(`Completed "${plainText(task.title)}"`);
			await reloadTasks();
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function deleteTask(task: Task) {
		const subs = subtasksOf(task.id).length;
		if (!(await confirmDialog(`Delete "${plainText(task.title)}"${subs ? ` and its ${subs} subtask${subs > 1 ? 's' : ''}` : ''}?`))) return;
		try {
			await api.tasks.delete(task.id);
			tasks = tasks.filter((t) => t.id !== task.id && t.parent_id !== task.id);
			completed = completed.filter((t) => t.id !== task.id);
			searchOpen = searchOpen.filter((t) => t.id !== task.id);
			searchDone = searchDone.filter((t) => t.id !== task.id);
			if (taskId === task.id) closeTask();
			toast.success('Task deleted');
			if (task.duration_minutes) await syncParentMinutes(task.parent_id, true);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function clearCompleted() {
		const ids = completed.map((t) => t.id);
		if (!ids.length) return;
		if (!(await confirmDialog(`Delete all ${ids.length} completed task${ids.length > 1 ? 's' : ''} here? This can't be undone.`))) return;
		try {
			await api.tasks.deleteCompleted(ids);
			const gone = new Set(ids);
			tasks = tasks.filter((t) => !gone.has(t.id) && !(t.parent_id && gone.has(t.parent_id)));
			completed = [];
			if (taskId && gone.has(taskId)) closeTask();
			toast.success('Completed tasks deleted');
			await reloadTasks();
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	// ── Projects and categories ────────────────────────────────────────────────

	function newProject() {
		projectDialog = { id: null, name: '', color: PROJECT_COLORS[(projects.length + 6) % PROJECT_COLORS.length].key };
	}

	function editProject(p: TaskProject) {
		projectDialog = { id: p.id, name: p.name, color: p.color ?? 'grey' };
	}

	async function saveProject() {
		if (!projectDialog || !projectDialog.name.trim()) return;
		const data = { name: projectDialog.name.trim(), color: projectDialog.color };
		try {
			if (projectDialog.id === null) {
				const created = await api.tasks.projects.create(data);
				projects = [...projects, created];
				go({ project: created.id });
			} else {
				const updated = await api.tasks.projects.update(projectDialog.id, data);
				projects = projects.map((p) => (p.id === updated.id ? updated : p));
			}
			projectDialog = null;
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function deleteProject(p: TaskProject) {
		const n = counts.projects[p.id] ?? 0;
		if (!(await confirmDialog(`Delete project "${p.name}"${n ? ` and its ${n} open task${n > 1 ? 's' : ''}` : ''}? Its done tasks go too.`))) return;
		try {
			await api.tasks.projects.delete(p.id);
			projects = projects.filter((x) => x.id !== p.id);
			categories = categories.filter((c) => c.project_id !== p.id);
			if (projectId === p.id) go({ view: 'inbox' });
			await reloadTasks();
			toast.success(`Deleted "${p.name}"`);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	/** Shows the new order at once, and puts the old one back if saving fails. */
	async function reorderProjects(ids: string[]) {
		const before = projects;
		const byId = new Map(projects.map((p) => [p.id, p]));
		projects = ids.map((id) => byId.get(id)).filter((p): p is TaskProject => !!p);
		try {
			projects = await api.tasks.projects.reorder(ids);
		} catch (e) {
			projects = before;
			toast.error(errorDetail(e));
		}
	}

	async function addCategory() {
		const name = newCategory.trim();
		if (!name || !projectId) return;
		try {
			const created = await api.tasks.categories.create({ project_id: projectId, name });
			categories = [...categories, created];
			newCategory = '';
			addingCategory = false;
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function renameCategory(c: TaskCategory) {
		const name = categoryName.trim();
		renamingCategory = null;
		if (!name || name === c.name) return;
		try {
			const updated = await api.tasks.categories.update(c.id, { project_id: c.project_id, name });
			categories = categories.map((x) => (x.id === updated.id ? updated : x));
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	async function deleteCategory(c: TaskCategory) {
		if (!(await confirmDialog(`Delete category "${c.name}"? Its tasks stay in the project.`))) return;
		try {
			await api.tasks.categories.delete(c.id);
			categories = categories.filter((x) => x.id !== c.id);
			await reloadTasks();
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	/** Its tasks and their subtasks go with it. */
	async function moveCategory(c: TaskCategory, target: TaskProject) {
		movingCategory = null;
		try {
			await api.tasks.categories.update(c.id, { project_id: target.id, name: c.name });
			categories = await api.tasks.categories.list();
			await reloadTasks();
			toast.success(`Moved "${c.name}" to ${target.name}`);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	function onCategoryMenu(c: TaskCategory, id: string) {
		if (id === 'rename') {
			renamingCategory = c.id;
			categoryName = c.name;
		} else if (id === 'move') {
			movingCategory = c;
		} else if (id === 'pin') {
			pin({ kind: 'category', id: c.id });
		} else {
			deleteCategory(c);
		}
	}

	// ── Filters ────────────────────────────────────────────────────────────────

	async function saveFilter(data: TaskFilterIn) {
		const editing = filterDialog;
		try {
			if (editing === 'new') {
				const created = await api.tasks.filters.create(data);
				filters = [...filters, created];
				filterDialog = null;
				go({ filter: created.id });
			} else if (editing) {
				const updated = await api.tasks.filters.update(editing.id, data);
				filters = filters.map((f) => (f.id === updated.id ? updated : f));
				filterDialog = null;
			}
		} catch (e) {
			toast.error(errorDetail(e));
			throw e;
		}
	}

	async function deleteFilter(f: TaskFilter) {
		if (!(await confirmDialog(`Delete filter "${f.name}"? Its tasks stay where they are.`))) return;
		try {
			await api.tasks.filters.delete(f.id);
			filters = filters.filter((x) => x.id !== f.id);
			if (filterId === f.id) go({});
			toast.success(`Deleted "${f.name}"`);
		} catch (e) {
			toast.error(errorDetail(e));
		}
	}

	function focusOnMount(node: HTMLElement) {
		node.focus();
	}

	function focusFirstButton(node: HTMLElement) {
		node.querySelector('button')?.focus();
	}

	// ── Keys ───────────────────────────────────────────────────────────────────

	function mainAddKey() {
		if (buckets) return buckets.find((b) => b.drop)?.key ?? 'main';
		return view === 'upcoming' ? `date:${tomorrow}` : 'main';
	}

	function onKeydown(e: KeyboardEvent) {
		if ((e.ctrlKey || e.metaKey) && !e.altKey && e.key.toLowerCase() === 'k') {
			if (projectDialog || filterDialog || movingCategory) return;
			e.preventDefault();
			if (taskId) closeTask();
			pane?.focusSearch();
			return;
		}
		if (e.key === 'Escape' && projectDialog) {
			projectDialog = null;
			return;
		}
		if (e.key === 'Escape' && movingCategory) {
			movingCategory = null;
			return;
		}
		if (e.key !== 'q' || e.ctrlKey || e.metaKey || e.altKey || e.defaultPrevented) return;
		const el = e.target as HTMLElement | null;
		if (el && (el.isContentEditable || ['INPUT', 'TEXTAREA', 'SELECT'].includes(el.tagName))) return;
		if (projectDialog || filterDialog || movingCategory || taskId || view === 'karma' || view === 'search') return;
		e.preventDefault();
		addingAt = mainAddKey();
	}

	/** Where the app-wide quick add puts a task by default while this page is open. */
	function quickAddDefaults() {
		// Phones' Browse tab isn't a list: a task added there goes to the Inbox, undated.
		if (browsing) return {};
		if (view === 'project' && projectId) {
			const m = /^#task-cat-([\w-]+)$/.exec(page.url.hash);
			const cat = m ? categories.find((c) => c.id === m[1] && c.project_id === projectId) : null;
			return { project_id: projectId, category_id: cat?.id ?? null };
		}
		if (view === 'filter') return filterBase;
		return view === 'today' ? { due_date: today } : view === 'upcoming' ? { due_date: tomorrow } : {};
	}

	onMount(() => {
		layouts = loadLayouts();
		grouping = loadGrouping();
		loadAll();
		const dropContext = setQuickAddContext(quickAddDefaults);
		// A task added from the app-wide quick add joins the list here.
		const stopListening = onTaskAdded((t) => {
			if (!tasks.some((x) => x.id === t.id)) tasks = [...tasks, t];
		});
		// The Tauri app's sync pulled changes made on another device.
		const stopChanged = onTasksChanged(async () => {
			await loadAll();
			if (completedShown) loadCompleted().catch(() => {});
			if (view === 'search') loadSearch().catch(() => {});
		});
		// Keeps "Today" and the overdue marks right past midnight and as times go by.
		const clock = setInterval(() => (now = new Date()), 60_000);
		return () => {
			clearInterval(clock);
			dropContext();
			stopListening();
			stopChanged();
			setTaskbarCount(0);
		};
	});
</script>

<svelte:window onkeydown={onKeydown} />

{#snippet row(task: Task, nested: boolean, withPlace: boolean, sorted = false)}
	<TaskRow
		{task}
		parent={parentOf(task)}
		sort={sorted ? sortApi : null}
		{subtasksOf}
		nested={(nested || (view === 'today' && !task.completed_at)) && settings.showSubtasks}
		subtaskFilter={view === 'today' ? dueSubtasks : null}
		showDoneSubtasks={settings.showCompleted}
		showNote={settings.showNotes}
		{now}
		place={view === 'filter' ? (filterFields?.has('place') ? placeOf(task) : null) : withPlace && settings.showPlace ? placeOf(task) : null}
		fields={view === 'filter' ? filterFields : null}
		editingId={inlineEditId}
		{editor}
		ontoggle={toggleTask}
		onopen={openTask}
		onedit={(t) => { addingAt = null; inlineEditId = t.id; }}
		onpin={isDesktop ? (t) => pin({ kind: 'task', id: t.id }) : null}
		ondelete={deleteTask}
	/>
{/snippet}

{#snippet editor(task: Task)}
	<TaskInlineEdit {task} {projects} {categories} onpatch={patchTask} ondone={() => (inlineEditId = null)} />
{/snippet}

{#snippet adder(key: string, defaults: AddDefaults)}
	{#if addingAt === key}
		<QuickAdd {projects} {categories} {defaults} onadd={addTask} oncancel={() => (addingAt = null)} />
	{:else}
		<button type="button" class="task-add" onclick={() => (addingAt = key)}>
			<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M12 5v14m7-7H5" /></svg>
			Add task
		</button>
	{/if}
{/snippet}

{#snippet group(title: string, list: Task[], opts: { overdue?: boolean; sub?: string } = {})}
	<section class="task-group">
		<h2 class="task-group-head" class:task-group-head-overdue={opts.overdue}>
			{title}
			{#if opts.sub}<span class="task-group-sub">{opts.sub}</span>{/if}
			<span class="task-group-count">{list.length}</span>
		</h2>
		{#each list as task (task.id)}
			{@render row(task, false, true)}
		{/each}
	</section>
{/snippet}

{#snippet bucketHead(b: { title: string; sub?: string; color?: string | null; overdue?: boolean }, count: number)}
	<h2 class="task-group-head" class:task-group-head-overdue={b.overdue}>
		{#if b.color}<span class="tasks-dot task-group-dot" style="background-color: {b.color}"></span>{/if}
		{b.title}
		{#if b.sub}<span class="task-group-sub">{b.sub}</span>{/if}
		<span class="task-group-count">{count}</span>
	</h2>
{/snippet}

{#snippet categoryHead(c: TaskCategory, count: number)}
	<div class="task-group-head task-category-head">
		{#if renamingCategory === c.id}
			<input
				class="input task-category-input"
				bind:value={categoryName}
				aria-label="Category name"
				use:focusOnMount
				onblur={() => renameCategory(c)}
				onkeydown={(e) => {
					if (e.key === 'Enter') { e.preventDefault(); renameCategory(c); }
					if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); renamingCategory = null; }
				}}
			/>
		{:else}
			<h2 class="task-category-name">{c.name}</h2>
			<span class="task-group-count">{count}</span>
			{#if minutes.categories[c.id]}
				<span class="task-group-count task-group-time" title="Open work in {c.name}">{formatDuration(minutes.categories[c.id])}</span>
			{/if}
			<DropdownMenu
				class="task-category-menu"
				triggerClass="tasks-icon-btn"
				ariaLabel="Category actions for {c.name}"
				items={[
					{ id: 'rename', label: 'Rename' },
					...(projects.length > 1 ? [{ id: 'move', label: 'Move to project…' }] : []),
					...(isDesktop ? [{ id: 'pin', label: 'Pin to desktop' }] : []),
					{ id: 'delete', label: 'Delete category', danger: true },
				]}
				onselect={(id) => onCategoryMenu(c, id)}
			>
				{#snippet icon()}
					<svg aria-hidden="true" class="w-3.5 h-3.5" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
				{/snippet}
			</DropdownMenu>
		{/if}
	</div>
{/snippet}

{#snippet categoryAdder()}
	{#if addingCategory}
		<div class="task-category-new">
			<input
				class="input"
				bind:value={newCategory}
				placeholder="Category name"
				aria-label="New category name"
				use:focusOnMount
				onkeydown={(e) => {
					if (e.key === 'Enter') { e.preventDefault(); addCategory(); }
					if (e.key === 'Escape') { e.preventDefault(); e.stopPropagation(); addingCategory = false; }
				}}
			/>
			<button type="button" class="btn btn-primary" onclick={addCategory} disabled={!newCategory.trim()}>Add</button>
			<button type="button" class="btn btn-ghost" onclick={() => (addingCategory = false)}>Cancel</button>
		</div>
	{:else}
		<button type="button" class="task-category-add" onclick={() => { addingCategory = true; newCategory = ''; }}>
			<span>+ Add category</span>
		</button>
	{/if}
{/snippet}

<div class="tasks-page" class:tasks-page-browsing={browsing}>
	<TasksPane
		bind:this={pane}
		query={searchQuery}
		onsearch={runSearch}
		{projects}
		{categories}
		{filters}
		{counts}
		{minutes}
		{view}
		{projectId}
		{filterId}
		{settings}
		{today}
		{browsing}
		onaddproject={newProject}
		oneditproject={editProject}
		ondeleteproject={deleteProject}
		onreorderprojects={reorderProjects}
		onaddfilter={() => (filterDialog = 'new')}
		oneditfilter={(f) => (filterDialog = f)}
		ondeletefilter={deleteFilter}
	/>

	<div class="tasks-main">
		<div class="tasks-content" class:tasks-content-wide={layout !== 'list'} data-sort-scope={layout === 'list' ? 'list' : undefined}>
			<header class="tasks-head">
				<div class="min-w-0">
					<h1 class="page-title tasks-title">
						{#if project}<span class="tasks-dot tasks-dot-lg" style="background-color: {projectColor(project.color)}"></span>{/if}
						{#if filter}<span class="tasks-dot tasks-dot-lg" style="background-color: {projectColor(filter.color)}"></span>{/if}
						{heading}
						{#if project && minutes.projects[project.id]}
							<span class="tasks-title-time" title="Open work in {project.name}">{formatDuration(minutes.projects[project.id])}</span>
						{/if}
					</h1>
					{#if view === 'today'}
						<p class="page-subtitle">{fromYmd(today).toLocaleDateString(undefined, { weekday: 'long', day: 'numeric', month: 'long' })}</p>
					{:else if view === 'karma'}
						<p class="page-subtitle">Your progress: tasks completed against your daily and weekly goals</p>
					{:else if view === 'search'}
						<p class="page-subtitle">{searchOpen.length} open, {searchDone.length} completed for "{searchQuery}"</p>
					{/if}
				</div>
				<div class="tasks-head-actions">
					{#if !loading && !error && groupShown}
						<GroupMenu value={groupBy} options={groupOptions} onchange={setGroup} />
					{/if}
					{#if !loading && !error && view !== 'karma' && view !== 'search'}
						<LayoutToggle {layout} options={layoutOptions} onchange={setLayout} />
					{/if}
					{#if view !== 'karma' && view !== 'search'}<SmartTips />{/if}
					{#if pinTarget}
						<button type="button" class="tasks-icon-btn" onclick={() => pin(pinTarget!)} title="Pin to desktop" aria-label="Pin to desktop">
							<svg aria-hidden="true" class="w-4 h-4" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.75" d="M12 17v5M7 17h10l-1.5-4.5V6a2 2 0 00-2-2h-3a2 2 0 00-2 2v6.5L7 17z" /></svg>
						</button>
					{/if}
					{#if filter}
						<DropdownMenu
							triggerClass="tasks-icon-btn"
							ariaLabel="Filter actions"
							items={[{ id: 'edit', label: 'Edit filter' }, { id: 'delete', label: 'Delete filter', danger: true }]}
							onselect={(id) => (id === 'edit' ? (filterDialog = filter) : deleteFilter(filter!))}
						>
							{#snippet icon()}
								<svg aria-hidden="true" class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
							{/snippet}
						</DropdownMenu>
					{/if}
					{#if project}
						<DropdownMenu
							triggerClass="tasks-icon-btn"
							ariaLabel="Project actions"
							items={[{ id: 'edit', label: 'Rename or recolour' }, { id: 'delete', label: 'Delete project', danger: true }]}
							onselect={(id) => (id === 'edit' ? editProject(project!) : deleteProject(project!))}
						>
							{#snippet icon()}
								<svg aria-hidden="true" class="w-4 h-4" fill="currentColor" viewBox="0 0 20 20"><path d="M6 10a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0zm6 0a2 2 0 11-4 0 2 2 0 014 0z" /></svg>
							{/snippet}
						</DropdownMenu>
					{/if}
				</div>
			</header>

			{#if view === 'search'}
				<!-- Phones: the pane with its box is gone once there are results, so this one takes over. -->
				<TaskSearchBox query={searchQuery} onsearch={runSearch} class="tasks-search-phone" autofocus />
			{/if}

			{#if loading}
				<div class="flex items-center gap-3 text-[var(--text-muted)]"><div class="spinner"></div>Loading…</div>
			{:else if error}
				<div class="error-box">Couldn't load tasks: {error}</div>
			{:else if view === 'karma'}
				<TaskKarma
					{today}
					dailyGoal={settings.dailyGoal}
					weeklyGoal={settings.weeklyGoal}
					ongoals={(dailyGoal, weeklyGoal) => changeSettings({ ...settings, dailyGoal, weeklyGoal })}
				/>
			{:else if view === 'search'}
				{#each searchOpen as task (task.id)}
					{@render row(task, false, true)}
				{/each}
				{#if searchDone.length}
					<section class="task-group">
						<h2 class="task-group-head">Completed <span class="task-group-count">{searchDone.length}</span></h2>
						{#each searchDone as task (task.id)}
							{@render row(task, false, true)}
						{/each}
					</section>
				{/if}
				{#if !searchOpen.length && !searchDone.length}
					<p class="tasks-empty">No tasks match "{searchQuery}".</p>
				{/if}
			{:else if view === 'project' && !project}
				<p class="tasks-empty">This project doesn't exist any more.</p>
			{:else if view === 'filter' && !filter}
				<p class="tasks-empty">This filter doesn't exist any more.</p>
			{:else if layout === 'board'}
				<div class="task-board">
					{#each columns as col (col.key)}
						<section
							id={col.category ? `task-cat-${col.category.id}` : undefined}
							class="task-col"
							class:task-col-over={dropOver === col.key}
							aria-label={col.title}
							data-sort-scope={col.key}
							data-sort-zone={view === 'project' ? col.category?.id ?? 'none' : undefined}
							ondragover={(e) => colDragOver(e, col)}
							ondragleave={(e) => colDragLeave(e, col)}
							ondrop={(e) => colDrop(e, col)}
						>
							{#if col.category}
								{@render categoryHead(col.category, col.tasks.length)}
							{:else}
								{@render bucketHead(col, col.tasks.length)}
							{/if}
							<div class="task-col-body">
								{#each col.tasks as task (task.id)}
									<div
										class="task-card"
										class:task-card-dragging={dragging?.id === task.id}
										draggable={inlineEditId === null || (inlineEditId !== task.id && !subtasksOf(task.id).some((s) => s.id === inlineEditId))}
										role="listitem"
										ondragstart={(e) => cardDragStart(e, task)}
										ondragend={cardDragEnd}
									>
										{@render row(task, sortable, !sortable, sortable)}
									</div>
								{/each}
								{#if col.add}{@render adder(col.add.key, col.add.defaults)}{/if}
							</div>
						</section>
					{/each}
					{#if buckets && !columns.some((c) => c.add)}
						<!-- No column takes new tasks (an empty view, or only Overdue / Next 7 days / Later). -->
						<section class="task-col" aria-label="New task">
							<h2 class="task-group-head">{buckets.length ? 'New task' : 'No tasks'}</h2>
							<div class="task-col-body">{@render adder('main', baseDefaults)}</div>
						</section>
					{/if}
					{#if view === 'project'}
						<div class="task-col task-col-new">{@render categoryAdder()}</div>
					{/if}
				</div>
			{:else if layout === 'calendar'}
				{#if addingAt === 'main' || addingAt?.startsWith('date:')}
					{@const date = addingAt.startsWith('date:') ? addingAt.slice(5) : null}
					<div class="tcal-adding">
						<p class="tcal-adding-label">{date ? `New task on ${dayLabel(date, now)}` : 'New task'}</p>
						{#key addingAt}
							<QuickAdd {projects} {categories} defaults={{ ...calendarDefaults, due_date: date }} onadd={addTask} oncancel={() => (addingAt = null)} />
						{/key}
					</div>
				{/if}
				<TaskCalendar
					tasks={calendarTasks}
					undated={calendarUndated}
					{today}
					onopen={openTask}
					ontoggle={toggleTask}
					onmove={moveToDate}
					onaddat={(date) => (addingAt = `date:${date}`)}
				/>
			{:else if buckets}
				{#each buckets as b (b.key)}
					<section class="task-group">
						{@render bucketHead(b, b.tasks.length)}
						{#each b.tasks as task (task.id)}
							{@render row(task, false, true)}
						{/each}
						{#if b.drop}{@render adder(b.key, b.defaults)}{/if}
					</section>
				{/each}
				<!-- When no bucket takes new tasks (none, or only Overdue / Next 7 days / Later), one Add task for the view. -->
				{#if !buckets.some((b) => b.drop)}
					{@render adder('main', baseDefaults)}
				{/if}
				{#if !buckets.length && addingAt !== 'main'}
					<p class="tasks-empty">{view === 'filter' ? 'No tasks match this filter.' : view === 'today' ? 'Nothing due today.' : 'Nothing coming up.'}</p>
				{/if}
			{:else if view === 'filter'}
				{#each filterTasks as task (task.id)}
					{@render row(task, false, true)}
				{/each}
				{@render adder('main', filterBase)}
				{#if !filterTasks.length && addingAt !== 'main'}
					<p class="tasks-empty">No tasks match this filter.</p>
				{/if}
			{:else if view === 'inbox'}
				{#each inboxTasks as task (task.id)}
					{@render row(task, true, false, true)}
				{/each}
				{@render adder('main', {})}
				{#if !inboxTasks.length && addingAt !== 'main'}
					<p class="tasks-empty">Nothing in the Inbox. Press <kbd>q</kbd> to add a task.</p>
				{/if}
			{:else if view === 'today'}
				{#if overdue.length}
					{@render group('Overdue', overdueRows, { overdue: true })}
				{/if}
				<section class="task-group">
					{#if overdue.length}
						<h2 class="task-group-head">{todayHeading(today)} <span class="task-group-count">{todayRows.length}</span></h2>
					{/if}
					{#each todayRows as task (task.id)}
						{@render row(task, false, true)}
					{/each}
					{@render adder('main', { due_date: today })}
				</section>
				{#if !overdue.length && !todayTasks.length && addingAt !== 'main'}
					<p class="tasks-empty">Nothing due today.</p>
				{/if}
			{:else if view === 'upcoming'}
				{#if !upcomingDays.some((d) => d.date === tomorrow)}
					{@const head = dayHeading(tomorrow)}
					<section class="task-group">
						<h2 class="task-group-head">
							{head.title}
							<span class="task-group-sub">{head.sub}</span>
						</h2>
						{@render adder(`date:${tomorrow}`, { due_date: tomorrow })}
					</section>
				{/if}
				{#each upcomingDays as day (day.date)}
					{@const head = dayHeading(day.date)}
					<section class="task-group">
						<h2 class="task-group-head">
							{head.title}
							<span class="task-group-sub">{head.sub}</span>
							<span class="task-group-count">{day.tasks.length}</span>
						</h2>
						{#each day.tasks as task (task.id)}
							{@render row(task, false, true)}
						{/each}
						{@render adder(`date:${day.date}`, { due_date: day.date })}
					</section>
				{/each}
				{#if later.length}
					{@render group('Later', later)}
				{/if}
			{:else if project}
				<div data-sort-zone="none">
					{#each uncategorised as task (task.id)}
						{@render row(task, true, false, true)}
					{/each}
					{@render adder('main', { project_id: project.id })}
				</div>

				{#each projectCategories as c (c.id)}
					{@const list = inCategory(c.id)}
					<section class="task-group" id="task-cat-{c.id}" data-sort-zone={c.id}>
						{@render categoryHead(c, list.length)}
						{#each list as task (task.id)}
							{@render row(task, true, false, true)}
						{/each}
						{@render adder(`cat:${c.id}`, { project_id: project.id, category_id: c.id })}
					</section>
				{/each}

				{@render categoryAdder()}
			{/if}

			{#if completedShown && (view === 'project' || view === 'inbox') && layout !== 'calendar'}
				<section class="task-group">
					<h2 class="task-group-head">
						Completed <span class="task-group-count">{completed.length}</span>
						{#if completed.length}
							<button type="button" class="ml-3 text-xs font-normal underline opacity-70 hover:opacity-100" onclick={clearCompleted}>Delete all</button>
						{/if}
					</h2>
					{#each completed as task (task.id)}
						{@render row(task, false, false)}
					{:else}
						<p class="tasks-empty">No completed tasks.</p>
					{/each}
				</section>
			{/if}
		</div>
	</div>

	<!-- Phones: the views along the bottom, like the pane on wider screens. -->
	<nav class="tasks-tabbar" aria-label="Views">
		{#each TABS as t (t.key)}
			{@const here = t.key === 'browse' ? browsing || view === 'project' || view === 'filter' || view === 'search' : !browsing && view === t.key}
			<a href="?view={t.key}" class="tasks-tab" aria-current={here ? 'page' : undefined}>
				{#if t.key === 'today'}
					<TodayIcon date={today} strokeWidth={here ? 2.1 : 1.75} />
				{:else}
					<svg aria-hidden="true" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width={here ? 2.1 : 1.75} d={t.icon} /></svg>
				{/if}
				<span>{t.label}</span>
			</a>
		{/each}
	</nav>
</div>

{#if drawerTask}
	<TaskDetail
		task={drawerTask}
		parent={drawerParent}
		subtasks={subtasksOf(drawerTask.id)}
		showDoneSubtasks={settings.showCompleted}
		{projects}
		{categories}
		onprev={neighbours.prev ? () => openTask(neighbours.prev!) : null}
		onnext={neighbours.next ? () => openTask(neighbours.next!) : null}
		sort={drawerTask.completed_at ? null : sortApi}
		onpin={isDesktop ? (t) => pin({ kind: 'task', id: t.id }) : null}
		onpatch={patchTask}
		ontoggle={toggleTask}
		onaddsubtask={addSubtask}
		onopen={openTask}
		ondelete={deleteTask}
		onclose={closeTask}
	/>
{/if}

{#if filterDialog}
	<FilterDialog
		filter={filterDialog === 'new' ? null : filterDialog}
		{projects}
		{categories}
		{tasks}
		{today}
		onsave={saveFilter}
		onclose={() => (filterDialog = null)}
	/>
{/if}

{#if movingCategory}
	{@const moving = movingCategory}
	<div class="modal-backdrop" onclick={() => (movingCategory = null)} aria-hidden="true"></div>
	<div class="modal-overlay">
		<div class="modal-panel max-w-sm" role="dialog" aria-modal="true" aria-labelledby="move-category-title">
			<div class="modal-header">
				<h2 id="move-category-title" class="modal-title">Move "{moving.name}" to</h2>
				<button onclick={() => (movingCategory = null)} class="modal-close" aria-label="Close">
					<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
				</button>
			</div>
			<div class="modal-body">
				<ul class="category-move-list" use:focusFirstButton>
					{#each projects.filter((p) => p.id !== moving.project_id) as p (p.id)}
						<li>
							<button type="button" class="category-move-option" onclick={() => moveCategory(moving, p)}>
								<span class="tasks-dot" style="background-color: {projectColor(p.color)}"></span>
								<span class="truncate">{p.name}</span>
							</button>
						</li>
					{/each}
				</ul>
			</div>
		</div>
	</div>
{/if}

{#if projectDialog}
	<div class="modal-backdrop" onclick={() => (projectDialog = null)} aria-hidden="true"></div>
	<div class="modal-overlay">
		<div class="modal-panel max-w-sm" role="dialog" aria-modal="true" aria-labelledby="project-dialog-title">
			<div class="modal-header">
				<h2 id="project-dialog-title" class="modal-title">{projectDialog.id === null ? 'New project' : 'Edit project'}</h2>
				<button onclick={() => (projectDialog = null)} class="modal-close" aria-label="Close">
					<svg aria-hidden="true" class="w-5 h-5" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M6 18L18 6M6 6l12 12" /></svg>
				</button>
			</div>
			<form class="modal-body" onsubmit={(e) => { e.preventDefault(); saveProject(); }}>
				<div>
					<label for="project-name" class="label">Name</label>
					<input id="project-name" class="input" bind:value={projectDialog.name} required use:focusOnMount />
				</div>
				<fieldset>
					<legend class="label">Colour</legend>
					<div class="project-swatches">
						{#each PROJECT_COLORS as c (c.key)}
							<label class="project-swatch" title={c.label}>
								<input type="radio" name="project-color" value={c.key} bind:group={projectDialog.color} class="sr-only" />
								<span class="project-swatch-dot" style="background-color: {c.value}"></span>
								<span class="sr-only">{c.label}</span>
							</label>
						{/each}
					</div>
				</fieldset>
				<div class="flex justify-end gap-3 pt-2">
					<button type="button" class="btn btn-secondary" onclick={() => (projectDialog = null)}>Cancel</button>
					<button type="submit" class="btn btn-primary" disabled={!projectDialog.name.trim()}>{projectDialog.id === null ? 'Add project' : 'Save'}</button>
				</div>
			</form>
		</div>
	</div>
{/if}
