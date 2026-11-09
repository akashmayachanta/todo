// Syncs tab groups to a Tasks server: each group becomes a task in the
// Learning project, YT category, and each of its videos becomes a subtask.
// The browser is the source of truth: a video in the group is an open
// subtask (added, or reopened if it was ticked off), and a video subtask
// whose tab is gone is ticked off. Nothing is ever deleted or renamed.

import { attachDurations, videoId } from './durations.js';

export const PROJECT = 'Learning';
export const CATEGORY = 'YT';

const CONFIG_KEY = 'tasksConfig';
const DEVICE_NAME = 'Tab Group Exporter';
// The server's limit on a subtask's title, counted on the plain text
// (SUBTASK_TITLE_MAX in tasks-core).
const SUBTASK_TITLE_MAX = 255;
// The server takes durations of 1 to 10080 minutes.
const DURATION_MAX = 10080;

export const DEFAULT_SERVER = 'http://localhost:8050';

// Whole minutes for a task's duration, or null when the length is unknown.
// The Tasks app adds the open subtasks' minutes up into the group task's total.
function minutes(seconds) {
  return seconds ? Math.min(DURATION_MAX, Math.max(1, Math.round(seconds / 60))) : null;
}

// A task's time as its subtasks' remaining total: the open ones' minutes,
// capped, or null once they are all done. Undefined when no subtask has a
// time (the task keeps its own) or the total is already right.
function parentMinutes(task, subtasks) {
  if (!subtasks.some((s) => s.duration_minutes)) return undefined;
  let total = 0;
  for (const s of subtasks) if (s.completed_at == null) total += s.duration_minutes ?? 0;
  const next = Math.min(total, DURATION_MAX) || null;
  return next === (task.duration_minutes ?? null) ? undefined : next;
}

// ---------- Config ----------

// { serverUrl, token, username, deviceId }. The password is never stored.
export async function loadConfig() {
  const { [CONFIG_KEY]: config = {} } = await chrome.storage.local.get(CONFIG_KEY);
  return config;
}

export async function saveConfig(config) {
  await chrome.storage.local.set({ [CONFIG_KEY]: config });
}

// "192.168.1.10:8050" → "http://192.168.1.10:8050/tasks-api". A URL that
// already has a path is kept as typed, for servers with another base path.
export function normalizeServerUrl(input) {
  let text = input.trim();
  if (!text) throw new Error('Enter the server address');
  if (!/^https?:\/\//i.test(text)) text = 'http://' + text;
  let u;
  try { u = new URL(text); } catch { throw new Error('That server address is not valid'); }
  const path = u.pathname.replace(/\/+$/, '');
  return u.origin + (path || '/tasks-api');
}

// The host permission Chrome must grant before the popup can reach the
// server. Left without a port, the pattern covers every port on that host.
export function originPattern(serverUrl) {
  const u = new URL(serverUrl);
  return `${u.protocol}//${u.hostname}/*`;
}

// ---------- API ----------

class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.status = status;
  }
}

async function request(serverUrl, token, path, { method = 'GET', body } = {}) {
  const headers = {};
  if (token) headers.Authorization = `Bearer ${token}`;
  if (body !== undefined) headers['Content-Type'] = 'application/json';
  let res;
  try {
    res = await fetch(serverUrl + path, {
      method,
      headers,
      body: body === undefined ? undefined : JSON.stringify(body)
    });
  } catch {
    throw new ApiError(`Could not reach the server at ${serverUrl}`, 0);
  }
  if (!res.ok) {
    let detail = '';
    try { detail = (await res.json()).detail || ''; } catch { /* not JSON */ }
    throw new ApiError(detail || `Server error ${res.status}`, res.status);
  }
  return res.status === 204 ? null : res.json();
}

// Calls the API with the saved token. A 401 means the token was revoked, so
// it is dropped and the popup asks the user to sign in again.
async function api(path, opts) {
  const config = await loadConfig();
  if (!config.token) throw new ApiError('Sign in first', 401);
  try {
    return await request(config.serverUrl, config.token, path, opts);
  } catch (err) {
    if (err.status === 401) {
      await saveConfig({ serverUrl: config.serverUrl, username: config.username });
      throw new ApiError('Signed out by the server. Sign in again.', 401);
    }
    throw err;
  }
}

export async function signIn(serverUrl, username, password) {
  const out = await request(serverUrl, null, '/auth/login', {
    method: 'POST',
    body: { username: username.trim(), password, device_name: DEVICE_NAME }
  });
  const config = { serverUrl, token: out.token, username: out.username, deviceId: out.device?.id };
  await saveConfig(config);
  return config;
}

// Revokes this device's token on the server when it can, then forgets it.
export async function signOut() {
  const config = await loadConfig();
  if (config.token && config.deviceId) {
    try {
      await request(config.serverUrl, config.token, `/auth/devices/${encodeURIComponent(config.deviceId)}/revoke`, { method: 'POST' });
    } catch { /* offline or already revoked: forgetting it locally is enough */ }
  }
  await saveConfig({ serverUrl: config.serverUrl, username: config.username });
}

// ---------- Matching ----------

const sameName = (a, b) => a.trim().toLowerCase() === b.trim().toLowerCase();

// Task titles are stored as typed, so "[label](url)" shows only "label".
const plainText = (s) => s.replace(/\[([^\]]*)\]\((https?:\/\/[^)\s]+)\)/g, '$1').trim();

function videoIdsIn(text) {
  const ids = [];
  for (const url of text.match(/https?:\/\/[^\s)\]]+/g) || []) {
    const id = videoId(url);
    if (id) ids.push(id);
  }
  return ids;
}

// "(3) How X works - YouTube" → "How X works". The app's link pattern has no
// escaped brackets, so [ ] in the label become ( ).
function videoLabel(tab, id) {
  const label = (tab.title || '')
    .replace(/^\(\d+\)\s+/, '')
    .replace(/\s+-\s+YouTube$/, '')
    .replace(/\[/g, '(')
    .replace(/\]/g, ')')
    .trim() || id;
  return [...label].length > SUBTASK_TITLE_MAX ? [...label].slice(0, SUBTASK_TITLE_MAX - 1).join('') + '…' : label;
}

async function findOrCreateProject() {
  const projects = await api('/projects');
  return projects.find((p) => sameName(p.name, PROJECT))
    || api('/projects', { method: 'POST', body: { name: PROJECT } });
}

async function findOrCreateCategory(projectId) {
  const categories = await api(`/categories?project_id=${encodeURIComponent(projectId)}`);
  return categories.find((c) => sameName(c.name, CATEGORY))
    || api('/categories', { method: 'POST', body: { project_id: projectId, name: CATEGORY } });
}

// ---------- Sync ----------

// Two groups with one title (say an open "Work" and a closed "Work") share a
// task, so their videos are synced together; otherwise one would tick off the
// other's videos.
function mergeByTitle(groups) {
  const merged = new Map();
  for (const group of groups) {
    const key = group.title.trim().toLowerCase();
    if (merged.has(key)) merged.get(key).tabs.push(...group.tabs);
    else merged.set(key, { title: group.title, tabs: [...group.tabs] });
  }
  return [...merged.values()];
}

// Returns { created, added, timed, reopened, ticked, skipped,
//   groups: [{ title, created, added, timed, reopened, ticked }] }.
// `timed` counts existing subtasks that had no duration and got one;
// `reopened` counts ticked-off subtasks set back to open;
// `ticked` counts video subtasks ticked off because their tab is gone;
// `skipped` counts groups with no YouTube videos and no task.
export async function addGroups(groups) {
  // Lengths come from the shared cache, so this is quick when the popup has
  // already fetched them; any it can't find leave that subtask without a time.
  await attachDurations(groups);

  const project = await findOrCreateProject();
  const category = await findOrCreateCategory(project.id);
  // One call: the open top-level tasks in the project, plus every subtask of
  // an open task, ticked off or not.
  const tasks = await api(`/?project_id=${encodeURIComponent(project.id)}`);

  const result = { created: 0, added: 0, timed: 0, reopened: 0, ticked: 0, skipped: 0, groups: [] };

  for (const group of mergeByTitle(groups)) {
    const videos = [];
    for (const tab of group.tabs) {
      const id = videoId(tab.url);
      if (id) videos.push({ id, tab });
    }

    let task = tasks.find((t) => t.parent_id == null && t.category_id === category.id
      && sameName(plainText(t.title), group.title));
    // A group whose videos are all closed still syncs an existing task, so
    // the last ones get ticked off; it just never creates one.
    if (!task && !videos.length) { result.skipped++; continue; }
    let created = false;
    if (!task) {
      task = await api('/', { method: 'POST', body: { title: group.title, project_id: project.id, category_id: category.id } });
      tasks.push(task);
      created = true;
      result.created++;
    }

    const subtasks = tasks.filter((t) => t.parent_id === task.id);
    const byId = new Map(); // video id -> subtask
    const byTitle = new Map(); // plain title, lower case -> subtask
    for (const sub of subtasks) {
      videoIdsIn(sub.title).forEach((id) => byId.set(id, sub));
      byTitle.set(plainText(sub.title).toLowerCase(), sub);
    }

    const kept = new Set(); // subtasks whose video is still in the group
    let added = 0;
    let timed = 0;
    let reopened = 0;
    let ticked = 0;
    // One at a time, in tab order, so the subtasks' positions follow the group.
    for (const { id, tab } of videos) {
      const label = videoLabel(tab, id);
      const duration = minutes(tab.seconds);
      const existing = byId.get(id) || byTitle.get(label.toLowerCase());
      if (existing) {
        kept.add(existing);
        const patch = {};
        // The browser wins: a video still in the group is still to watch,
        // even if it was ticked off in the app.
        if (existing.completed_at != null) patch.completed_at = null;
        // Fill in a missing time, e.g. on videos added before times were sent.
        // A time already set is left alone, in case it was edited in the app.
        if (duration && existing.duration_minutes == null) patch.duration_minutes = duration;
        if (Object.keys(patch).length) {
          Object.assign(existing, await api(`/${encodeURIComponent(existing.id)}`, { method: 'POST', body: patch }));
          if ('completed_at' in patch) reopened++;
          if ('duration_minutes' in patch) timed++;
        }
        continue;
      }
      const body = { title: `[${label}](https://www.youtube.com/watch?v=${id})`, parent_id: task.id };
      if (duration) body.duration_minutes = duration;
      const sub = await api('/', { method: 'POST', body });
      tasks.push(sub);
      kept.add(sub);
      byId.set(id, sub);
      byTitle.set(label.toLowerCase(), sub);
      added++;
    }

    // Closing a video's tab means it was watched, so its subtask is ticked
    // off. Only subtasks that link a video are touched; anything typed in
    // the app by hand is left alone.
    for (const sub of subtasks) {
      if (kept.has(sub) || sub.completed_at != null || !videoIdsIn(sub.title).length) continue;
      Object.assign(sub, await api(`/${encodeURIComponent(sub.id)}`, {
        method: 'POST', body: { completed_at: new Date().toISOString() }
      }));
      ticked++;
    }

    // The Tasks app stores a task's time as its open subtasks' total and
    // recomputes it whenever it changes a subtask; the server never does, so
    // after a sync the extension saves it the same way (parentMinutes in the
    // app's shared.ts).
    const total = parentMinutes(task, tasks.filter((t) => t.parent_id === task.id));
    if (total !== undefined) {
      Object.assign(task, await api(`/${encodeURIComponent(task.id)}`, { method: 'POST', body: { duration_minutes: total } }));
    }

    result.added += added;
    result.timed += timed;
    result.reopened += reopened;
    result.ticked += ticked;
    result.groups.push({ title: group.title, created, added, timed, reopened, ticked });
  }
  return result;
}
