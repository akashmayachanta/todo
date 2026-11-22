// Syncs tab groups to a Tasks server: each group becomes a task in the
// Learning project, YT section, and each of its videos becomes a subtask.
// The browser is the source of truth: a video in the group is an open
// subtask (added, or reopened if it was ticked off), and a video subtask
// whose tab is gone is ticked off. A group removed from the archive has its
// task closed. Nothing is ever deleted or renamed.
//
// It talks to the server's MCP endpoint with an MCP token made in Tasks →
// Settings → AI; there is no sign-in.

import { loadArchive } from './archive.js';
import { attachDurations, videoId } from './durations.js';

export const PROJECT = 'Learning';
export const SECTION = 'YT';
// The popup's pseudo-group for tabs outside any group. It is never in the
// archive, so it must not count as a removed group.
export const UNGROUPED_TITLE = 'Ungrouped tabs';

const CONFIG_KEY = 'tasksConfig';
// { "<serverUrl>": ["<group title, lower case>", ...] }: the groups this
// extension has synced to each server. Only these can be closed as removed,
// so another browser's groups in the same account are left alone.
const SYNCED_KEY = 'tasksSynced';
// Set once by background.js on updating from a version without SYNCED_KEY,
// so the first sync also closes the tasks of groups removed before then.
const CLEANUP_KEY = 'tasksCleanupPending';
// MCP tokens start with this; device tokens, from before 1.8, with the other.
const MCP_PREFIX = 'mcp_';
const DEVICE_PREFIX = 'dt_';
// The server's limit on a subtask's title, counted on the plain text
// (SUBTASK_TITLE_MAX in tasks-core).
const SUBTASK_TITLE_MAX = 255;
// The server takes durations of 1 to 10080 minutes.
const DURATION_MAX = 10080;
// list_tasks pages; the server takes up to 500.
const PAGE = 500;

export const DEFAULT_SERVER = 'http://localhost:8050';

// Whole minutes for a task's duration, or null when the length is unknown.
// The Tasks app adds the open subtasks' minutes up into the group task's total.
function minutes(seconds) {
  return seconds ? Math.min(DURATION_MAX, Math.max(1, Math.round(seconds / 60))) : null;
}

// ---------- Config ----------

// A fresh install starts with an empty record; an update from a version
// without one asks the next sync for a one-time cleanup instead.
export async function initSynced(reason) {
  const { [SYNCED_KEY]: synced } = await chrome.storage.local.get(SYNCED_KEY);
  if (synced) return;
  if (reason === 'install') await chrome.storage.local.set({ [SYNCED_KEY]: {} });
  else if (reason === 'update') await chrome.storage.local.set({ [CLEANUP_KEY]: true });
}

// { serverUrl, token }: an MCP token. A config from before 1.8 holds a device
// token (dt_…) instead, which counts as not connected; connecting revokes it.
export async function loadConfig() {
  const { [CONFIG_KEY]: config = {} } = await chrome.storage.local.get(CONFIG_KEY);
  return config;
}

export function isConnected(config) {
  return !!config.serverUrl && typeof config.token === 'string' && config.token.startsWith(MCP_PREFIX);
}

async function saveConfig(config) {
  await chrome.storage.local.set({ [CONFIG_KEY]: config });
}

// "192.168.1.10:8050" → "http://192.168.1.10:8050/tasks-api". The MCP address
// copied from Settings → AI (…/tasks-api/mcp) works too. A URL with another
// path is kept as typed, for servers with another base path.
export function normalizeServerUrl(input) {
  let text = input.trim();
  if (!text) throw new Error('Enter the server address');
  if (!/^https?:\/\//i.test(text)) text = 'http://' + text;
  let u;
  try { u = new URL(text); } catch { throw new Error('That server address is not valid'); }
  const path = u.pathname.replace(/\/+$/, '').replace(/\/mcp$/i, '');
  return u.origin + (path || '/tasks-api');
}

// The host permission Chrome must grant before the popup can reach the
// server. Left without a port, the pattern covers every port on that host.
export function originPattern(serverUrl) {
  const u = new URL(serverUrl);
  return `${u.protocol}//${u.hostname}/*`;
}

// "http://192.168.1.10:8050/tasks-api" → "192.168.1.10:8050", for the popup.
export function serverLabel(serverUrl) {
  try { return new URL(serverUrl).host; } catch { return serverUrl; }
}

// ---------- MCP ----------
// The server's MCP endpoint ({serverUrl}/mcp, tasks-server's mcp.rs) is
// stateless: each call is one JSON-RPC message, with no session to open.

class ApiError extends Error {
  constructor(message, status) {
    super(message);
    this.status = status;
  }
}

let rpcId = 0;

async function rpc(serverUrl, token, method, params) {
  let res;
  try {
    res = await fetch(`${serverUrl}/mcp`, {
      method: 'POST',
      headers: {
        Authorization: `Bearer ${token}`,
        'Content-Type': 'application/json',
        Accept: 'application/json, text/event-stream'
      },
      body: JSON.stringify({ jsonrpc: '2.0', id: ++rpcId, method, params })
    });
  } catch {
    throw new ApiError(`Could not reach the server at ${serverUrl}`, 0);
  }
  if (!res.ok) {
    let detail = '';
    try { detail = (await res.json()).detail || ''; } catch { /* not JSON */ }
    if (res.status === 404 || res.status === 405) detail = 'This Tasks server has no MCP endpoint yet. Update the server.';
    throw new ApiError(detail || `Server error ${res.status}`, res.status);
  }
  const msg = await res.json();
  if (msg.error) throw new ApiError(msg.error.message || 'The server refused the call', 400);
  return msg.result;
}

// Runs one tool. A tool's own failure (bad input, not found, a read-only
// token) comes back as isError, with a message to show.
async function callTool(serverUrl, token, name, args = {}) {
  const result = await rpc(serverUrl, token, 'tools/call', { name, arguments: args });
  const text = result?.content?.find((c) => c.type === 'text')?.text ?? '';
  if (result?.isError) throw new ApiError(text || `${name} failed`, 422);
  if (result?.structuredContent !== undefined) return result.structuredContent;
  try { return JSON.parse(text); } catch { return text; }
}

// A tool with the saved token. A 401 means it expired or was deleted, so it
// is dropped and the popup asks for a new one.
async function tool(name, args) {
  const config = await loadConfig();
  if (!isConnected(config)) throw new ApiError('Connect to Tasks first', 401);
  try {
    return await callTool(config.serverUrl, config.token, name, args);
  } catch (err) {
    if (err.status === 401) {
      await saveConfig({ serverUrl: config.serverUrl });
      throw new ApiError('Tasks refused the MCP token: it has expired or was deleted. Paste a new one.', 401);
    }
    throw err;
  }
}

// Checks that the token can add tasks, then keeps it. A device token from an
// older version is revoked on the server, best effort.
export async function connect(serverUrl, token) {
  token = token.trim();
  if (!token.startsWith(MCP_PREFIX)) {
    throw new Error('Paste an MCP token (it starts with mcp_). Make one in Tasks → Settings → AI.');
  }
  let listed;
  try {
    listed = await rpc(serverUrl, token, 'tools/list', {});
  } catch (err) {
    if (err.status === 401) throw new Error('Tasks refused that token: it is wrong, expired or deleted.');
    throw err;
  }
  if (!listed?.tools?.some((t) => t.name === 'create_task')) {
    throw new Error('That token is read-only. Make a Read & write token in Tasks → Settings → AI.');
  }
  const previous = await loadConfig();
  if (previous.token?.startsWith(DEVICE_PREFIX) && previous.deviceId && previous.serverUrl) {
    try {
      await fetch(`${previous.serverUrl}/auth/devices/${encodeURIComponent(previous.deviceId)}/revoke`, {
        method: 'POST',
        headers: { Authorization: `Bearer ${previous.token}` }
      });
    } catch { /* offline or already revoked: forgetting it is enough */ }
  }
  const config = { serverUrl, token };
  await saveConfig(config);
  return config;
}

// Forgets the token here. It stays valid until it expires or is deleted in
// Tasks → Settings → AI.
export async function disconnect() {
  const config = await loadConfig();
  await saveConfig({ serverUrl: config.serverUrl });
}

// ---------- Matching ----------

const titleKey = (s) => s.trim().toLowerCase();
const sameName = (a, b) => titleKey(a) === titleKey(b);

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

// MCP can't make projects or sections, so they must exist in Tasks already.
async function findProjectAndSection() {
  const { projects } = await tool('list_projects');
  const project = projects.find((p) => sameName(p.name, PROJECT));
  const { sections } = project ? await tool('list_sections', { project_id: project.id }) : { sections: [] };
  const section = sections.find((c) => sameName(c.name, SECTION));
  if (!project || !section) {
    throw new ApiError(`In Tasks, make a project called "${PROJECT}" with a section called "${SECTION}", then try again.`, 422);
  }
  return { project, section };
}

// Every open task in the project with its subtasks, a page at a time, so a
// long watch list isn't cut short (a missed subtask would be added twice).
async function projectTasks(projectId) {
  const tasks = [];
  for (;;) {
    const page = await tool('list_tasks', { project_id: projectId, limit: PAGE, offset: tasks.length });
    tasks.push(...page.tasks);
    if (!page.more || !page.tasks.length) return tasks;
  }
}

// ---------- Sync ----------

// The groups synced to this server. Before 1.8 the record was kept per
// "<serverUrl>|<username>"; an MCP token has no username, and these are this
// browser's groups whichever account they went to, so those lists fold in.
function syncedTitles(byServer, serverUrl) {
  if (byServer[serverUrl]) return byServer[serverUrl];
  const titles = new Set();
  for (const [key, list] of Object.entries(byServer)) {
    if (key.startsWith(`${serverUrl}|`)) list.forEach((t) => titles.add(t));
  }
  return [...titles];
}

// Two groups with one title (say an open "Work" and a closed "Work") share a
// task, so their videos are synced together; otherwise one would tick off the
// other's videos.
function mergeByTitle(groups) {
  const merged = new Map();
  for (const group of groups) {
    const key = titleKey(group.title);
    if (merged.has(key)) merged.get(key).tabs.push(...group.tabs);
    else merged.set(key, { title: group.title, tabs: [...group.tabs] });
  }
  return [...merged.values()];
}

async function patchTask(task, body) {
  Object.assign(task, await tool('update_task', { id: task.id, ...body }));
}

// Today's date here, not the server's: a repeating task moves on from it.
function localDay() {
  const d = new Date();
  const two = (n) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${two(d.getMonth() + 1)}-${two(d.getDate())}`;
}

// Ticking and unticking say what happened and the server applies the rules:
// a subtask's time leaves its task's total when ticked and returns when
// reopened. The task object is updated; its parent's copy here is not.
async function completeTask(task) {
  Object.assign(task, await tool('complete_task', { id: task.id, today: localDay() }));
}

async function reopenTask(task) {
  Object.assign(task, await tool('reopen_task', { id: task.id }));
}

// A removed group's videos count as watched: its open video subtasks are
// ticked off, and the task is completed once nothing in it is left open.
// Completing a task on the server also ticks its open subtasks, so a task
// with a hand-typed subtask still open stays open, with its time updated.
async function closeRemoved(task, tasks) {
  const subtasks = tasks.filter((t) => t.parent_id === task.id);
  for (const sub of subtasks) {
    if (sub.completed_at != null || !videoIdsIn(sub.title).length) continue;
    await completeTask(sub);
  }
  // Each tick already took that subtask's time off the task's total.
  if (subtasks.every((s) => s.completed_at != null)) await completeTask(task);
}

// Returns { created, added, timed, reopened, ticked, skipped, removed,
//   groups: [{ title, created, added, timed, reopened, ticked }] }.
// `timed` counts existing subtasks that had no duration and got one;
// `reopened` counts ticked-off subtasks set back to open;
// `ticked` counts video subtasks ticked off because their tab is gone;
// `skipped` counts groups with no YouTube videos and no task;
// `removed` counts tasks closed because their group left the archive.
export async function addGroups(groups) {
  // Lengths come from the shared cache, so this is quick when the popup has
  // already fetched them; any it can't find leave that subtask without a time.
  await attachDurations(groups);

  const config = await loadConfig();
  const stored = await chrome.storage.local.get([SYNCED_KEY, CLEANUP_KEY]);
  const syncedByServer = stored[SYNCED_KEY] || {};
  const synced = new Set(syncedTitles(syncedByServer, config.serverUrl));
  const saveSynced = () => chrome.storage.local.set({
    [SYNCED_KEY]: { ...syncedByServer, [config.serverUrl]: [...synced] }
  });

  const { project, section } = await findProjectAndSection();
  // The open top-level tasks in the project, plus every subtask of an open
  // task, ticked off or not.
  const tasks = await projectTasks(project.id);
  const groupTask = (key) => tasks.find((t) => t.parent_id == null && t.section_id === section.id
    && titleKey(plainText(t.title)) === key);

  const result = { created: 0, added: 0, timed: 0, reopened: 0, ticked: 0, skipped: 0, removed: 0, groups: [] };

  for (const group of mergeByTitle(groups)) {
    const videos = [];
    for (const tab of group.tabs) {
      const id = videoId(tab.url);
      if (id) videos.push({ id, tab });
    }

    let task = groupTask(titleKey(group.title));
    // A group whose videos are all closed still syncs an existing task, so
    // the last ones get ticked off; it just never creates one.
    if (!task && !videos.length) { result.skipped++; continue; }
    let created = false;
    if (!task) {
      task = await tool('create_task', { title: group.title, project_id: project.id, section_id: section.id });
      tasks.push(task);
      created = true;
      result.created++;
    }
    if (group.title !== UNGROUPED_TITLE) synced.add(titleKey(group.title));

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
        // The browser wins: a video still in the group is still to watch,
        // even if it was ticked off in the app.
        if (existing.completed_at != null) {
          await reopenTask(existing);
          reopened++;
        }
        // Fill in a missing time, e.g. on videos added before times were sent.
        // A time already set is left alone, in case it was edited in the app.
        if (duration && existing.duration_minutes == null) {
          await patchTask(existing, { duration_minutes: duration });
          timed++;
        }
        continue;
      }
      const body = { title: `[${label}](https://www.youtube.com/watch?v=${id})`, parent_id: task.id };
      if (duration) body.duration_minutes = duration;
      const sub = await tool('create_task', body);
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
      await completeTask(sub);
      ticked++;
    }

    // The task's time, its open subtasks' total, is kept by the server.

    result.added += added;
    result.timed += timed;
    result.reopened += reopened;
    result.ticked += ticked;
    result.groups.push({ title: group.title, created, added, timed, reopened, ticked });
  }
  await saveSynced();

  // Groups synced before but no longer in the archive (removed with ×,
  // "Forget closed", or renamed) are checked whatever was sent, since the
  // archive covers every window, open and closed.
  const archive = await loadArchive();
  const present = new Set(Object.values(archive).map((e) => titleKey(e.title)));
  const candidates = new Set(synced);
  if (stored[CLEANUP_KEY]) {
    // One-time: groups removed before this was tracked. Any group task with
    // a video subtask counts as one this extension made.
    for (const t of tasks) {
      if (t.parent_id != null || t.section_id !== section.id) continue;
      if (tasks.some((s) => s.parent_id === t.id && videoIdsIn(s.title).length)) {
        candidates.add(titleKey(plainText(t.title)));
      }
    }
  }
  for (const key of candidates) {
    if (present.has(key) || key === titleKey(UNGROUPED_TITLE)) continue;
    const task = groupTask(key);
    // Already completed or deleted in the app: the server no longer lists it.
    if (task) {
      await closeRemoved(task, tasks);
      result.removed++;
    }
    synced.delete(key);
    await saveSynced();
  }
  if (stored[CLEANUP_KEY]) await chrome.storage.local.remove(CLEANUP_KEY);
  return result;
}
