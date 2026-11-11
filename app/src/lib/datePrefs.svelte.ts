/**
 * Settings → General → Date & time, for the plain modules that show and read
 * dates ($lib/taskParse). It is a rune, so a template that shows a time redraws
 * when the choice changes. $lib/stores/taskSettings keeps it in step with the
 * saved settings.
 */

/** "13:00" or "1:00 PM". */
export type TimeFormat = '24h' | '12h';
/** How "12/11" is read: day first (12 Nov) or month first (11 Dec). */
export type DateFormat = 'dmy' | 'mdy';

export interface DatePrefs {
	timeFormat: TimeFormat;
	dateFormat: DateFormat;
	/** The first day of a week in the calendar and date picker, 0 (Sunday) to 6. */
	weekStart: number;
	/** The day "next week" means, in the week after this one. */
	nextWeek: number;
	/** The day "weekend" means: 6 (Saturday) or 0 (Sunday). */
	weekend: number;
	/** Dates, times and repeats typed in a task's name are read. */
	smartDates: boolean;
}

/** What the device's locale does, for a first run. */
export function localeDatePrefs(): Pick<DatePrefs, 'timeFormat' | 'dateFormat'> {
	try {
		const fmt = new Intl.DateTimeFormat(undefined, { hour: 'numeric' }).resolvedOptions();
		const parts = new Intl.DateTimeFormat().formatToParts(new Date(2000, 11, 31));
		const dayFirst = parts.findIndex((p) => p.type === 'day') < parts.findIndex((p) => p.type === 'month');
		return { timeFormat: fmt.hour12 ? '12h' : '24h', dateFormat: dayFirst ? 'dmy' : 'mdy' };
	} catch {
		return { timeFormat: '24h', dateFormat: 'dmy' };
	}
}

export const DEFAULT_DATE_PREFS: DatePrefs = { ...localeDatePrefs(), weekStart: 1, nextWeek: 1, weekend: 6, smartDates: true };

export const datePrefs = $state<DatePrefs>({ ...DEFAULT_DATE_PREFS });

/** The hour and minute options for toLocale*String, in the chosen format. */
export function clockOptions(): Intl.DateTimeFormatOptions {
	return { hour: 'numeric', minute: '2-digit', hourCycle: datePrefs.timeFormat === '12h' ? 'h12' : 'h23' };
}

/** An instant as "3 Nov, 17:00" (or "3 Nov, 5:00 PM"). */
export function formatInstant(iso: string): string {
	return new Date(iso).toLocaleString(undefined, { day: 'numeric', month: 'short', ...clockOptions() });
}

/** An instant in full, for a tooltip. */
export function formatInstantLong(iso: string): string {
	return new Date(iso).toLocaleString(undefined, { day: 'numeric', month: 'long', year: 'numeric', ...clockOptions() });
}

/** How many days a weekday (0 Sunday … 6) is past the first day of its week. */
export function daysIntoWeek(weekday: number): number {
	return (weekday - datePrefs.weekStart + 7) % 7;
}
