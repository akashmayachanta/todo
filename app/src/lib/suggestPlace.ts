// Files a new task by what its title says. A small Naive Bayes classifier is
// trained on the user's own tasks: each task is an example of its project (or,
// once done there, of the Inbox, which then means "leave it be"), and of its
// category (or of "no category") within the project. The project and category
// names count as examples too, along with words that usually go with such a name
// (WORD_GROUPS), so a new category gets suggestions before it holds any tasks.
// Everything runs on the device, from the tasks already synced to it, so every
// device learns the same things.

import type { Task, TaskCategory, TaskProject } from '$lib/api';
import { linkParts } from '$lib/links';

/** A place the task's words point to. category_id null means the project alone. */
export interface PlaceGuess {
	project_id: string;
	category_id: string | null;
	/** The words in the title that weighed most, as typed. */
	because: string[];
}

/** How sure it must be before filing the task without being asked. */
const PROJECT_SURE = 0.75;
const CATEGORY_SURE = 0.65;
/** A task this many days old counts half as much as a new one. */
const HALF_LIFE_DAYS = 180;
const MIN_WEIGHT = 0.2;
/** A project or category name counts as this many tasks; the words that go with it, as one. */
const NAME_WEIGHT = 2;
const GROUP_WEIGHT = 1;
/** Smoothing: low, so one word seen only in one place is enough to go on. */
const ALPHA = 0.1;
/** The class for a task in a project but in none of its categories. */
const NO_CATEGORY = '';
/** The class for a task left in the Inbox. */
const INBOX = '';

const STOP = new Set(
	('a an the and or but of to in on at for from with by as is are was be been it its this that these those ' +
		'my me i we our you your he she they them his her their do does did done get got go make need needs ' +
		'about into up out over some any all just also so if then than not no yes will would can could should ' +
		'today tomorrow tonight morning evening week month year next new')
		.split(' '),
);

/** The root of a word, so "meetings", "meeting" and "meet" are one. */
function stem(w: string): string {
	if (w.length > 4 && w.endsWith('ies')) w = `${w.slice(0, -3)}y`;
	else if (w.length > 3 && w.endsWith('s') && !w.endsWith('ss')) w = w.slice(0, -1);
	if (w.length > 5 && w.endsWith('ing')) return w.slice(0, -3);
	if (w.length > 4 && w.endsWith('ed')) return w.slice(0, -2);
	return w;
}

/**
 * Words that go together. A project or category named with any of them (say "Paisa"
 * or "Bills") starts out knowing the rest, until the user's own tasks say more.
 */
const WORD_GROUPS = [
	'money finance finances paisa bills bill pay payment payments rent emi loan bank tax taxes insurance salary invoice ' +
		'upi recharge electricity water gas credit card budget expense expenses fees fee investment invest mutual fund sip ' +
		'stock shares refund transfer account accounts lic premium',
	'home house household chores clean cleaning laundry repair plumber electrician kitchen cook cooking garden furniture',
	'shopping shop groceries grocery buy order amazon flipkart milk vegetables fruits supermarket',
	'health fitness gym doctor dentist medicine medicines workout exercise yoga walk hospital checkup diet sleep',
	'meeting meetings call sync standup discuss discussion catchup demo interview agenda review',
	'work office job deadline report email mail client deploy release presentation slides jira ticket ppt',
	'learning learn study course read book books tutorial video watch youtube yt practice notes lecture class',
	'travel trip flight train bus hotel booking visa passport pack packing',
	'family mom dad mother father amma nanna birthday gift wedding friends kids school',
].map((g) => g.split(' ').map((w) => stem(w)));

interface Feature {
	key: string;
	/** The word as typed, for `because`. */
	word: string;
}

/** The words of a title (stemmed, without the filler), pairs of them, and the sites it links to. */
export function features(title: string): Feature[] {
	const out: Feature[] = [];
	const words: Feature[] = [];
	for (const part of linkParts(title)) {
		if (part.url) {
			try {
				const host = new URL(part.url).hostname.replace(/^www\./, '');
				out.push({ key: `@${host}`, word: host });
			} catch { /* not a usable address */ }
			if (part.text === part.url) continue;
		}
		for (const raw of part.text.replace(/['’]/g, '').split(/[^\p{L}\p{N}]+/u)) {
			const w = raw.toLowerCase();
			if (w.length < 2 || STOP.has(w) || /^\d+$/.test(w)) continue;
			words.push({ key: stem(w), word: raw });
		}
	}
	for (let i = 0; i < words.length; i++) {
		out.push(words[i]);
		if (i > 0) out.push({ key: `${words[i - 1].key} ${words[i].key}`, word: `${words[i - 1].word} ${words[i].word}` });
	}
	return out;
}

interface ClassStats {
	docs: number;
	words: number;
	counts: Map<string, number>;
}

/** Multinomial Naive Bayes over weighted examples. */
class Bayes {
	private classes = new Map<string, ClassStats>();
	private vocab = new Set<string>();
	private docs = 0;

	add(label: string, feats: Feature[], weight: number) {
		if (!feats.length) return;
		let c = this.classes.get(label);
		if (!c) this.classes.set(label, (c = { docs: 0, words: 0, counts: new Map() }));
		c.docs += weight;
		this.docs += weight;
		for (const f of feats) {
			c.counts.set(f.key, (c.counts.get(f.key) ?? 0) + weight);
			c.words += weight;
			this.vocab.add(f.key);
		}
	}

	/** The likeliest class and how likely, or null when none of the words were seen before. */
	best(feats: Feature[]): { label: string; p: number; because: string[] } | null {
		const known = feats.filter((f) => this.vocab.has(f.key));
		if (!known.length || !this.classes.size) return null;
		const v = this.vocab.size;
		const k = this.classes.size;
		const scores: { label: string; score: number; c: ClassStats }[] = [];
		for (const [label, c] of this.classes) {
			let score = Math.log((c.docs + 1) / (this.docs + k));
			const denom = Math.log(c.words + ALPHA * v);
			for (const f of known) score += Math.log((c.counts.get(f.key) ?? 0) + ALPHA) - denom;
			scores.push({ label, score, c });
		}
		scores.sort((a, b) => b.score - a.score);
		const top = scores[0];
		// It must have seen at least one of these words in the winner itself.
		if (!known.some((f) => top.c.counts.has(f.key))) return null;
		let sum = 0;
		for (const s of scores) sum += Math.exp(s.score - top.score);
		const because = known
			.filter((f) => top.c.counts.has(f.key) && !f.key.includes(' '))
			.sort((a, b) => (top.c.counts.get(b.key) ?? 0) - (top.c.counts.get(a.key) ?? 0))
			.map((f) => f.word)
			.filter((w, i, all) => all.indexOf(w) === i)
			.slice(0, 3);
		return { label: top.label, p: 1 / sum, because };
	}
}

export interface PlaceModel {
	projects: Bayes;
	/** By project id; only projects with categories. */
	categories: Map<string, Bayes>;
	/** Which projects and categories still exist. */
	projectIds: Set<string>;
	categoryProject: Map<string, string>;
}

/** The words of the groups a name belongs to. */
function groupFeatures(name: Feature[]): Feature[] {
	const keys = new Set(name.map((f) => f.key));
	const out = new Set<string>();
	for (const group of WORD_GROUPS) {
		if (group.some((w) => keys.has(w))) for (const w of group) if (!keys.has(w)) out.add(w);
	}
	return [...out].map((key) => ({ key, word: key }));
}

function ageWeight(createdAt: string, now: number): number {
	const t = Date.parse(createdAt);
	if (!Number.isFinite(t)) return 1;
	const days = Math.max(0, (now - t) / 86_400_000);
	return Math.max(MIN_WEIGHT, 0.5 ** (days / HALF_LIFE_DAYS));
}

/** Learns from the tasks; subtasks teach nothing, as they go where their task is. */
export function trainPlaces(
	tasks: Task[],
	projects: TaskProject[],
	categories: TaskCategory[],
	now = Date.now(),
): PlaceModel {
	const model: PlaceModel = {
		projects: new Bayes(),
		categories: new Map(),
		projectIds: new Set(projects.map((p) => p.id)),
		categoryProject: new Map(categories.map((c) => [c.id, c.project_id])),
	};
	const inProject = (projectId: string) => {
		let b = model.categories.get(projectId);
		if (!b) model.categories.set(projectId, (b = new Bayes()));
		return b;
	};
	for (const p of projects) {
		const feats = features(p.name);
		model.projects.add(p.id, feats, NAME_WEIGHT);
		model.projects.add(p.id, groupFeatures(feats), GROUP_WEIGHT);
	}
	for (const c of categories) {
		if (!model.projectIds.has(c.project_id)) continue;
		const feats = features(c.name);
		const group = groupFeatures(feats);
		model.projects.add(c.project_id, feats, NAME_WEIGHT / 2);
		model.projects.add(c.project_id, group, GROUP_WEIGHT / 2);
		inProject(c.project_id).add(c.id, feats, NAME_WEIGHT);
		inProject(c.project_id).add(c.id, group, GROUP_WEIGHT);
	}
	for (const t of tasks) {
		if (t.parent_id) continue;
		const feats = features(t.title);
		const w = ageWeight(t.created_at, now);
		if (!t.project_id) {
			// An open one may just not be sorted yet; one done in the Inbox belonged there.
			if (t.completed_at) model.projects.add(INBOX, feats, w);
			continue;
		}
		if (!model.projectIds.has(t.project_id)) continue;
		model.projects.add(t.project_id, feats, w);
		const category = t.category_id && model.categoryProject.get(t.category_id) === t.project_id ? t.category_id : NO_CATEGORY;
		// "No category" is only worth learning in a project that has some.
		if (category !== NO_CATEGORY || model.categories.has(t.project_id)) inProject(t.project_id).add(category, feats, w);
	}
	return model;
}

/**
 * Where a title belongs, when it is sure enough: a project and one of its categories,
 * or the project alone when no category fits. With `projectId`, the task already has
 * its project and only a category of it is guessed.
 */
export function guessPlace(model: PlaceModel, title: string, projectId: string | null = null): PlaceGuess | null {
	const feats = features(title);
	if (!feats.length) return null;
	let project = projectId;
	let because: string[] = [];
	if (project === null) {
		const best = model.projects.best(feats);
		if (!best || best.label === INBOX || best.p < PROJECT_SURE) return null;
		project = best.label;
		because = best.because;
	}
	const inProject = model.categories.get(project);
	const best = inProject ? inProject.best(feats) : null;
	if (best && best.label !== NO_CATEGORY && best.p >= CATEGORY_SURE) {
		return { project_id: project, category_id: best.label, because: because.length ? because : best.because };
	}
	return projectId === null ? { project_id: project, category_id: null, because } : null;
}
