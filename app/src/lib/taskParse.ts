/**
 * Smart input for tasks. parseQuickAdd reads one line such as
 * "Pay rent every month p1 #Home !1h" into a task: the date, time, reminder,
 * priority, recurrence and place it names, and the title left once those are
 * taken out. Dates are the user's local calendar dates (`YYYY-MM-DD`), times
 * local `HH:MM`; only a reminder is an instant (ISO, UTC).
 */
import * as chrono from 'chrono-node';
import type { Task, TaskCategory, TaskPriority, TaskProject } from '$lib/api';
import { blankLinks } from '$lib/links';

export type TokenKind = 'date' | 'priority' | 'project' | 'category' | 'recurrence' | 'reminder' | 'duration';

/** A piece of the input that was read as something other than the title; [start, end) in the text. */
export interface ParsedToken {
	kind: TokenKind;
	start: number;
	end: number;
	text: string;
	/** `#Name` or `/Name` that matches no project or category. */
	unknown?: boolean;
}

export interface QuickAdd {
	title: string;
	due_date: string | null;
	due_time: string | null;
	/** How long it takes, from "for 45m" / "for 1h30m". */
	duration_minutes: number | null;
	remind_at: string | null;
	priority: TaskPriority | null;
	recurrence: string | null;
	project_id: string | null;
	category_id: string | null;
	tokens: ParsedToken[];
}

// ── Local dates ─────────────────────────────────────────────────────────────

const pad = (n: number) => String(n).padStart(2, '0');

/** `YYYY-MM-DD` of a Date in local time. */
export function ymd(d: Date): string {
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** Local midnight of a `YYYY-MM-DD`. */
export function fromYmd(s: string): Date {
	const [y, m, d] = s.split('-').map(Number);
	return new Date(y, m - 1, d);
}

export function addDays(s: string, days: number): string {
	const d = fromYmd(s);
	d.setDate(d.getDate() + days);
	return ymd(d);
}

/** Whole days from a to b (b - a); DST-safe. */
export function daysBetween(a: string, b: string): number {
	const [ya, ma, da] = a.split('-').map(Number);
	const [yb, mb, db] = b.split('-').map(Number);
	return Math.round((Date.UTC(yb, mb - 1, db) - Date.UTC(ya, ma - 1, da)) / 86_400_000);
}

function addMonths(s: string, months: number): string {
	const [y, m, d] = s.split('-').map(Number);
	const target = new Date(y, m - 1 + months, 1);
	// Clamp the 31st to the last day of a shorter month.
	const last = new Date(target.getFullYear(), target.getMonth() + 1, 0).getDate();
	target.setDate(Math.min(d, last));
	return ymd(target);
}

/** The instant a local date and time stand for, as ISO (UTC). */
export function localInstant(date: string, time: string | null): string {
	const d = fromYmd(date);
	const [h, m] = (time ?? '09:00').split(':').map(Number);
	d.setHours(h, m, 0, 0);
	return d.toISOString();
}

// ── Recurrence ──────────────────────────────────────────────────────────────

const WEEKDAYS = ['sunday', 'monday', 'tuesday', 'wednesday', 'thursday', 'friday', 'saturday'];
const DAY_WORD = '(?:mon|tue|tues|wed|thu|thur|thurs|fri|sat|sun)(?:day)?|tuesday|wednesday|thursday|saturday';
const RECURRENCE_RE = new RegExp(
	String.raw`\bevery\s+(?:` +
		String.raw`(other\s+(?:day|week|month|year))` +
		String.raw`|(\d+)\s+(days?|weeks?|months?|years?)` +
		String.raw`|(weekday|workday|day|week|month|year)` +
		String.raw`|((?:${DAY_WORD})(?:\s*(?:,|and|&)\s*(?:${DAY_WORD}))*)` +
		String.raw`)\b`,
	'i',
);

function weekdayName(word: string): string {
	const w = word.toLowerCase().slice(0, 3);
	return WEEKDAYS.find((d) => d.startsWith(w))!;
}

/** The normalised phrase for a recurrence match, e.g. "every monday, thursday". */
function normaliseMatch(m: RegExpExecArray): string {
	const [, other, n, unit, simple, days] = m;
	if (other) {
		const u = other.split(/\s+/)[1].toLowerCase();
		return u === 'day' ? 'every 2 days' : `every 2 ${u}s`;
	}
	if (n) {
		const count = Number(n);
		const u = unit.toLowerCase().replace(/s$/, '');
		return count === 1 ? `every ${u}` : `every ${count} ${u}s`;
	}
	if (simple) return `every ${simple.toLowerCase() === 'workday' ? 'weekday' : simple.toLowerCase()}`;
	const names = days.split(/\s*(?:,|and|&)\s*/i).filter(Boolean).map(weekdayName);
	const unique = WEEKDAYS.filter((d) => names.includes(d));
	return `every ${unique.join(', ')}`;
}

/** "every mon and thu" → "every monday, thursday"; null when it isn't a recurrence. */
export function normalizeRecurrence(text: string): string | null {
	const t = text.trim().replace(/\s+/g, ' ');
	if (!t) return null;
	const phrase = /^every\b/i.test(t) ? t : `every ${t}`;
	const m = RECURRENCE_RE.exec(phrase);
	return m && m.index === 0 && m[0].length === phrase.length ? normaliseMatch(m) : null;
}

interface Rule {
	unit: 'day' | 'weekday' | 'week' | 'month' | 'year' | 'days-of-week';
	every: number;
	days: number[];
}

function rule(recurrence: string): Rule | null {
	const r = recurrence.toLowerCase().replace(/^every\s+/, '').trim();
	if (r === 'weekday') return { unit: 'weekday', every: 1, days: [] };
	const n = /^(\d+)\s+(day|week|month|year)s$/.exec(r);
	if (n) return { unit: n[2] as Rule['unit'], every: Number(n[1]), days: [] };
	if (['day', 'week', 'month', 'year'].includes(r)) return { unit: r as Rule['unit'], every: 1, days: [] };
	const days = r.split(/\s*,\s*/).map((d) => WEEKDAYS.indexOf(d));
	if (days.length && days.every((d) => d >= 0)) return { unit: 'days-of-week', every: 1, days };
	return null;
}

/** The first date on or after `from` the rule falls on. */
function firstOn(r: Rule, from: string): string {
	if (r.unit === 'weekday') {
		let d = from;
		while ([0, 6].includes(fromYmd(d).getDay())) d = addDays(d, 1);
		return d;
	}
	if (r.unit === 'days-of-week') {
		let d = from;
		while (!r.days.includes(fromYmd(d).getDay())) d = addDays(d, 1);
		return d;
	}
	return from;
}

/** The first date on or after `from` (default today) a recurrence falls on; null when it isn't understood. */
export function firstOccurrence(recurrence: string, from = ymd(new Date())): string | null {
	const r = rule(recurrence);
	return r ? firstOn(r, from) : null;
}

function step(r: Rule, date: string): string {
	switch (r.unit) {
		case 'day':   return addDays(date, r.every);
		case 'week':  return addDays(date, 7 * r.every);
		case 'month': return addMonths(date, r.every);
		case 'year':  return addMonths(date, 12 * r.every);
		default:      return firstOn(r, addDays(date, 1));
	}
}

/**
 * The next due date of a recurring task after `due_date`, rolled on past today
 * so finishing an overdue "every day" doesn't leave it overdue. Null when the
 * recurrence isn't understood. core/src/recurrence.rs is the same rule for the
 * notification's Complete button, so change the two together.
 */
export function nextOccurrence(recurrence: string, due_date: string | null, now = new Date()): string | null {
	const r = rule(recurrence);
	if (!r) return null;
	const today = ymd(now);
	const from = due_date ?? today;
	if (r.unit === 'month' || r.unit === 'year') {
		// Counted from the first date, so the 31st comes back after a shorter month.
		const months = r.unit === 'month' ? r.every : 12 * r.every;
		let k = 1;
		while (addMonths(from, k * months) <= today) k++;
		return addMonths(from, k * months);
	}
	let next = step(r, from);
	while (next <= today) next = step(r, next);
	return next;
}

// ── Quick add ───────────────────────────────────────────────────────────────

const PRIORITY_RE = /(^|\s)(p[1-4])(?=\s|$)/gi;
// "for 45m", "for 1h", "for 1h30m", "for 1.5 hours", "for 90 min"
const DURATION_RE = /(^|\s)(for\s+(?:\d+(?:\.\d+)?\s*(?:h|hrs?|hours?)(?:\s*\d+\s*(?:m|mins?|minutes?))?|\d+\s*(?:m|mins?|minutes?)))(?=\s|$|[.,;])/gi;
const RELATIVE_REMINDER_RE = /(^|\s)(!(\d+)\s*(m|min|mins|h|hr|hrs|d)\b)/gi;

/** Minutes in "for 1h30m", "1.5 hours", "90 min"; null when there are none. */
export function parseDuration(text: string): number | null {
	const h = /(\d+(?:\.\d+)?)\s*h/i.exec(text);
	const m = /(\d+)\s*m/i.exec(h ? text.slice(h.index + h[0].length) : text);
	const total = Math.round((h ? Number(h[1]) * 60 : 0) + (m ? Number(m[1]) : 0));
	return total > 0 && total <= 10080 ? total : null;
}

/** "45m", "1h", "1h 30m". */
export function formatDuration(minutes: number): string {
	const h = Math.floor(minutes / 60);
	const m = minutes % 60;
	return h ? (m ? `${h}h ${m}m` : `${h}h`) : `${m}m`;
}

/** "HH:MM" plus some minutes, wrapping past midnight. */
export function addMinutes(time: string, minutes: number): string {
	const [h, m] = time.split(':').map(Number);
	const t = (((h * 60 + m + minutes) % 1440) + 1440) % 1440;
	return `${pad(Math.floor(t / 60))}:${pad(t % 60)}`;
}

/** The longest name in `names` the text starts with, ended by a space or the end. */
function longestName<T extends { name: string }>(text: string, items: T[]): T | null {
	const lower = text.toLowerCase();
	let best: T | null = null;
	for (const item of items) {
		const name = item.name.toLowerCase();
		if (!lower.startsWith(name)) continue;
		const after = text[name.length];
		if (after !== undefined && !/\s/.test(after)) continue;
		if (!best || name.length > best.name.length) best = item;
	}
	return best;
}

function overlaps(tokens: ParsedToken[], start: number, end: number): boolean {
	return tokens.some((t) => start < t.end && end > t.start);
}

/** The text with the token ranges blanked out, so offsets stay the same. */
function mask(text: string, tokens: ParsedToken[]): string {
	const chars = text.split(''); // UTF-16 units, as the offsets are, so an emoji doesn't shift them.
	for (const t of tokens) for (let i = t.start; i < t.end; i++) chars[i] = ' ';
	return chars.join('');
}

// "12/11" is read the way the user's locale writes dates: day first unless it puts the month first.
const dayFirst = (() => {
	try {
		const parts = new Intl.DateTimeFormat().formatToParts(new Date(2000, 11, 31));
		return parts.findIndex((p) => p.type === 'day') < parts.findIndex((p) => p.type === 'month');
	} catch {
		return false;
	}
})();
const dates = (dayFirst ? chrono.en.GB : chrono.casual).clone();

// Short forms: "tod" (today), "tom" / "tmr" / "tmrw" (tomorrow).
dates.parsers.push({
	pattern: () => /\b(tod|tom|tmrw?)\b/i,
	extract: (ctx, m) => {
		const d = new Date(ctx.refDate);
		if (m[1].toLowerCase() !== 'tod') d.setDate(d.getDate() + 1);
		const c = ctx.createParsingComponents();
		c.assign('day', d.getDate());
		c.assign('month', d.getMonth() + 1);
		c.assign('year', d.getFullYear());
		return c;
	},
});

// "10p", "9:30a": a time with a one-letter meridiem.
dates.parsers.push({
	pattern: () => /\b(1[0-2]|0?[1-9])(?::([0-5]\d))?\s?([ap])\b/i,
	extract: (ctx, m) => {
		const pm = m[3].toLowerCase() === 'p';
		const hour = (Number(m[1]) % 12) + (pm ? 12 : 0);
		const c = ctx.createParsingComponents();
		c.assign('hour', hour);
		c.assign('minute', Number(m[2] ?? 0));
		c.assign('meridiem', pm ? 1 : 0);
		return c;
	},
});

const timeOf = (d: Date) => `${pad(d.getHours())}:${pad(d.getMinutes())}`;

function hasDay(c: chrono.ParsedComponents): boolean {
	return c.isCertain('day') || c.isCertain('weekday');
}

/**
 * Reads a quick-add line. `#Name` picks a project or a category and `/Category` a
 * category, for where the task goes, and `#Project / Category` both (names may have
 * spaces; the longest match wins, and a project wins a tie), `p1`–`p4` the priority,
 * `every …` a recurrence, `!5pm` or `!tomorrow 9am` a reminder at that time,
 * `!30m` / `!1h` / `!1d` a reminder that long before the due time (or from now
 * when there is no due date), and any other date phrase chrono understands the
 * due date and time. `contextProjectId` is where a category is looked for
 * first when no project is named. `contextDate` is the day a time alone
 * ("5pm") falls on, e.g. the Upcoming group or calendar day being added to;
 * without one a time alone is today (or tomorrow once it has passed).
 */
export function parseQuickAdd(
	line: string,
	projects: TaskProject[],
	categories: TaskCategory[],
	now = new Date(),
	contextProjectId: string | null = null,
	contextDate: string | null = null,
): QuickAdd {
	// A link is left as it is: its label and address aren't dates or places.
	const text = blankLinks(line);
	const tokens: ParsedToken[] = [];
	const out: QuickAdd = {
		title: '', due_date: null, due_time: null, duration_minutes: null, remind_at: null, priority: null,
		recurrence: null, project_id: null, category_id: null, tokens,
	};
	const add = (kind: TokenKind, start: number, end: number, unknown = false) =>
		tokens.push({ kind, start, end, text: text.slice(start, end), ...(unknown ? { unknown } : {}) });

	// Projects first, then categories, so the order of the tokens doesn't matter.
	const places: { at: number; gap: number; rest: string; hash: boolean; project: TaskProject | null }[] = [];
	for (const m of text.matchAll(/(^|\s)#(?=\S)/g)) {
		const at = m.index! + m[1].length;
		const rest = text.slice(at + 1);
		const project = longestName(rest, projects);
		const category = longestName(rest, categories);
		// #Name is whichever name is longer; a project wins a tie.
		if (!project || (category && category.name.length > project.name.length)) {
			places.push({ at, gap: 0, rest, hash: true, project });
		} else if (out.project_id === null) {
			out.project_id = project.id;
			add('project', at, at + 1 + project.name.length);
		}
	}
	// "#Work / Backend" is read like "#Work /Backend"; anywhere else a spaced slash is just a slash.
	for (const m of text.matchAll(/(^|\s)\/(\s?)(?=\p{L})/gu)) {
		const at = m.index! + m[1].length;
		if (m[2] && !tokens.some((t) => t.kind === 'project' && !t.unknown && t.end <= at && !text.slice(t.end, at).trim())) continue;
		places.push({ at, gap: m[2].length, rest: text.slice(at + 1 + m[2].length), hash: false, project: null });
	}
	places.sort((a, b) => a.at - b.at);

	let unknownCategory: { start: number; end: number } | null = null;
	for (const { at, gap, rest, hash, project } of places) {
		// In the named or current project first; everywhere only when no project is named.
		const inProject = out.project_id ?? contextProjectId;
		const pool = inProject !== null ? categories.filter((c) => c.project_id === inProject) : categories;
		const category = longestName(rest, pool) ?? (out.project_id === null ? longestName(rest, categories) : null);
		const end = at + 1 + gap + rest.split(/\s/)[0].length;
		if (category) {
			if (out.category_id !== null) continue;
			out.category_id = category.id;
			out.project_id ??= category.project_id;
			add('category', at, at + 1 + gap + category.name.length);
		} else if (hash) {
			if (!project) add('project', at, end, true);
		} else if (!unknownCategory) {
			unknownCategory = { start: at, end };
		}
	}
	if (unknownCategory) add('category', unknownCategory.start, unknownCategory.end, true);

	// p1–p4
	for (const m of text.matchAll(PRIORITY_RE)) {
		const start = m.index! + m[1].length;
		if (overlaps(tokens, start, start + 2)) continue;
		out.priority = Number(m[2][1]) as TaskPriority;
		add('priority', start, start + 2);
	}

	// every …
	const rec = RECURRENCE_RE.exec(mask(text, tokens));
	if (rec) {
		out.recurrence = normaliseMatch(rec);
		add('recurrence', rec.index, rec.index + rec[0].length);
	}

	// for 45m, for 1h30m: how long it takes
	for (const m of text.matchAll(DURATION_RE)) {
		const start = m.index! + m[1].length;
		if (overlaps(tokens, start, start + m[2].length)) continue;
		const minutes = parseDuration(m[2]);
		if (!minutes) continue;
		out.duration_minutes = minutes;
		add('duration', start, start + m[2].length);
		break;
	}

	// !30m, !1h: before the due time
	let reminderOffset: number | null = null;
	for (const m of text.matchAll(RELATIVE_REMINDER_RE)) {
		const start = m.index! + m[1].length;
		if (overlaps(tokens, start, start + m[2].length)) continue;
		const unit = m[4].toLowerCase()[0];
		reminderOffset = Number(m[3]) * (unit === 'm' ? 60_000 : unit === 'h' ? 3_600_000 : 86_400_000);
		add('reminder', start, start + m[2].length);
		break;
	}

	// !5pm, !tomorrow 9am: a reminder at that time
	let reminderAt: chrono.ParsedComponents | null = null;
	if (reminderOffset === null) {
		for (const m of text.matchAll(/(^|\s)!(?=\S)/g)) {
			const at = m.index! + m[1].length;
			if (overlaps(tokens, at, at + 1)) continue;
			const tail = mask(text, tokens).slice(at + 1);
			const [hit] = dates.parse(tail, now, { forwardDate: true });
			if (hit && hit.index === 0) {
				reminderAt = hit.start;
				add('reminder', at, at + 1 + hit.text.length);
				break;
			}
		}
	}

	// Any other date phrase is the due date.
	let timeOnly = false;
	for (const hit of dates.parse(mask(text, tokens), now, { forwardDate: true })) {
		const c = hit.start;
		const day = hasDay(c);
		// A lone month name ("may", "march") is far more often a word of the title.
		if (!day && !c.isCertain('hour') && !c.isCertain('year')) continue;
		if (out.due_date === null) {
			const date = c.date();
			// A time alone lands on the day being added to, when there is one.
			out.due_date = !day && contextDate ? contextDate : ymd(date);
			out.due_time = c.isCertain('hour') ? timeOf(date) : null;
			timeOnly = !day;
			add('date', hit.index, hit.index + hit.text.length);
		} else if (out.due_time === null && c.isCertain('hour') && !day) {
			out.due_time = timeOf(c.date());
			add('date', hit.index, hit.index + hit.text.length);
		}
	}

	// A recurrence starts on its first day unless a date says otherwise.
	if (out.recurrence) {
		const r = rule(out.recurrence)!;
		if (out.due_date === null || timeOnly) out.due_date = firstOn(r, contextDate ?? ymd(now));
	}

	if (reminderOffset !== null) {
		const base = out.due_date ? new Date(localInstant(out.due_date, out.due_time)).getTime() : now.getTime();
		out.remind_at = new Date(out.due_date ? base - reminderOffset : base + reminderOffset).toISOString();
	} else if (reminderAt) {
		// "!9am" with a due date reminds on that day.
		const at = reminderAt.date();
		out.remind_at = !hasDay(reminderAt) && out.due_date
			? localInstant(out.due_date, timeOf(at))
			: at.toISOString();
	}

	tokens.sort((a, b) => a.start - b.start);
	out.title = mask(line, tokens.filter((t) => !t.unknown)).replace(/\s+/g, ' ').trim();
	return out;
}

/** One line of a pasted list. `minutes` comes from a trailing " · 17:52" or " · 1:39:43". */
export interface PastedLine {
	text: string;
	minutes: number | null;
}

const LIST_MARK_RE = /^\s*(?:[-*+•]\s+|\d+[.)]\s+)?(?:\[[ xX]\]\s+)?/;
const CLOCK_TAIL_RE = /\s+·\s+(?:(\d+):)?(\d{1,2}):(\d{2})\s*$/;

/** The items of pasted text, one per line, without list marks. Blank lines and "## headings" are skipped. */
export function pastedLines(text: string): PastedLine[] {
	const lines: PastedLine[] = [];
	for (const raw of text.split(/\r?\n/)) {
		if (!raw.trim() || /^\s*#{1,6}\s/.test(raw)) continue;
		const item = raw.replace(LIST_MARK_RE, '');
		const clock = CLOCK_TAIL_RE.exec(item);
		const seconds = clock ? Number(clock[1] ?? 0) * 3600 + Number(clock[2]) * 60 + Number(clock[3]) : 0;
		const minutes = Math.max(1, Math.round(seconds / 60));
		lines.push({
			text: (clock ? item.slice(0, clock.index) : item).trim(),
			minutes: clock && minutes <= 10080 ? minutes : null,
		});
	}
	return lines.filter((l) => l.text);
}

/**
 * Reads an edited title the way the quick add reads a new line, but only for what
 * was typed into it: a date, priority or place already in the saved `original` title
 * stays plain text, so fixing a typo in "Meeting on Friday" doesn't move its date.
 * The title is the edited text with only the newly read parts taken out.
 */
export function parseEdit(
	text: string,
	original: string,
	projects: TaskProject[],
	categories: TaskCategory[],
	now = new Date(),
	contextProjectId: string | null = null,
	contextDate: string | null = null,
): QuickAdd {
	const parse = (t: string) => parseQuickAdd(t, projects, categories, now, contextProjectId, contextDate);
	const key = (t: ParsedToken) => `${t.kind}:${t.text.toLowerCase()}`;
	const before = new Set(parse(original).tokens.map(key));
	const all = parse(text);
	const old = all.tokens.filter((t) => before.has(key(t)));
	const out = old.length ? parse(mask(text, old)) : all;
	for (const t of out.tokens) t.text = text.slice(t.start, t.end);
	out.title = mask(text, out.tokens.filter((t) => !t.unknown)).replace(/\s+/g, ' ').trim();
	return out;
}

// ── Labels ──────────────────────────────────────────────────────────────────

export interface DueLabel {
	label: string;
	overdue: boolean;
	today: boolean;
}

/** "17:00" in the user's locale. */
export function formatTime(time: string): string {
	const [h, m] = time.split(':').map(Number);
	const d = new Date();
	d.setHours(h, m, 0, 0);
	return d.toLocaleTimeString(undefined, { hour: 'numeric', minute: '2-digit' });
}

/** "5:00 pm", or "5:00 pm – 5:45 pm" with a duration. */
export function timeRange(time: string, duration?: number | null): string {
	return duration ? `${formatTime(time)} – ${formatTime(addMinutes(time, duration))}` : formatTime(time);
}

/** "Today", "Tomorrow", "Yesterday", "Fri" (within a week), "3 Nov", "3 Nov 2027". */
export function dayLabel(date: string, now = new Date()): string {
	const diff = daysBetween(ymd(now), date);
	if (diff === 0) return 'Today';
	if (diff === 1) return 'Tomorrow';
	if (diff === -1) return 'Yesterday';
	const d = fromYmd(date);
	if (diff > 1 && diff < 7) return d.toLocaleDateString(undefined, { weekday: 'short' });
	const sameYear = d.getFullYear() === now.getFullYear();
	return d.toLocaleDateString(undefined, { day: 'numeric', month: 'short', ...(sameYear ? {} : { year: 'numeric' }) });
}

/** "Thu 5 Nov", in the order the user's locale writes it. */
export function shortDate(date: string): string {
	return fromYmd(date).toLocaleDateString(undefined, { weekday: 'short', day: 'numeric', month: 'short' }).replace(/,/g, '');
}

/** A section heading for today: "Today · Thu 5 Nov". */
export function todayHeading(today: string): string {
	return `Today · ${shortDate(today)}`;
}

/** The due label of a task, with its time, and whether it is overdue; null when it has no date. */
export function describeDue(task: Pick<Task, 'due_date' | 'due_time'> & { duration_minutes?: number | null }, now = new Date()): DueLabel | null {
	if (!task.due_date) return null;
	const today = ymd(now);
	const overdue = task.due_date < today || (task.due_date === today && !!task.due_time && task.due_time < timeOf(now));
	const label = dayLabel(task.due_date, now) + (task.due_time ? ` ${timeRange(task.due_time, task.duration_minutes)}` : '');
	return { label, overdue, today: task.due_date === today };
}
