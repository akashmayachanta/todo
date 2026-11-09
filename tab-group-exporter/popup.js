import { GROUP_COLORS, snapshot, forgetClosed, forgetGroup } from './archive.js';
import { attachDurations, formatDuration, groupTime, videoId } from './durations.js';
import qrcode from './vendor/qrcode.js';
import { addGroups, DEFAULT_SERVER, loadConfig, normalizeServerUrl, originPattern, signIn, signOut } from './tasks.js';

qrcode.stringToBytes = qrcode.stringToBytesFuncs['UTF-8'];

const $ = (id) => document.getElementById(id);
const state = { groups: [] };

// ---------- 1. Collect ----------

async function collectGroups({ currentWindowOnly, includeUngrouped, includeClosed }) {
  // Refresh the archive first so anything just closed shows up as closed.
  const archive = await snapshot();

  const [tabs, groups] = await Promise.all([
    chrome.tabs.query(currentWindowOnly ? { currentWindow: true } : {}),
    chrome.tabGroups.query({})
  ]);

  const byId = new Map(groups.map((g) => [g.id, {
    title: g.title || 'Untitled group',
    color: g.color,
    open: true,
    tabs: []
  }]));

  tabs.sort((a, b) => a.windowId - b.windowId || a.index - b.index);

  const ordered = [];
  const ungrouped = [];

  for (const tab of tabs) {
    const entry = { title: tab.title || tab.url, url: tab.url || tab.pendingUrl || '' };
    const group = byId.get(tab.groupId);
    if (group) {
      if (group.tabs.length === 0) ordered.push(group);
      group.tabs.push(entry);
    } else {
      ungrouped.push(entry);
    }
  }

  if (includeClosed) {
    const closed = Object.entries(archive)
      .filter(([, e]) => !e.open)
      .sort(([, a], [, b]) => b.lastSeen - a.lastSeen)
      .map(([key, { title, color, tabs, lastSeen }]) => ({ key, title, color, tabs, lastSeen, open: false }));
    ordered.push(...closed);
  }

  if (includeUngrouped && ungrouped.length) {
    ordered.push({ title: 'Ungrouped tabs', color: null, open: true, tabs: ungrouped });
  }
  return ordered;
}

// ---------- 2. Format ----------

const escapeMd = (s) => s.replace(/([\[\]\\])/g, '\\$1');

const formatters = {
  markdown: (groups) => groups.map((g) => {
    const { seconds, partial } = groupTime(g);
    const total = seconds ? ` · ${formatDuration(seconds)}${partial ? '+' : ''}` : '';
    return `## ${g.title} (${g.tabs.length}${total})\n` + g.tabs.map((t) =>
      `- [${escapeMd(t.title)}](${t.url})${t.seconds ? ` · ${formatDuration(t.seconds)}` : ''}`
    ).join('\n');
  }).join('\n\n'),

  text: (groups) => groups.map((g) =>
    `${g.title} (${g.tabs.length})\n` + g.tabs.map((t) => `  ${t.title}\n  ${t.url}`).join('\n\n')
  ).join('\n\n'),

  urls: (groups) => groups.map((g) =>
    `# ${g.title}\n` + g.tabs.map((t) => t.url).join('\n')
  ).join('\n\n'),

  json: (groups) => JSON.stringify({
    exportedAt: new Date().toISOString(),
    groups: groups.map(({ title, color, open, tabs }) => ({
      title, color, open,
      tabs: tabs.map(({ title, url, seconds }) => (seconds ? { title, url, seconds } : { title, url }))
    }))
  }, null, 2)
};

// ---------- 3. Render ----------

function renderPills(groups) {
  const box = $('groups');
  box.replaceChildren();
  for (const g of groups) {
    const pill = document.createElement('span');
    const color = g.color ? (GROUP_COLORS[g.color] || GROUP_COLORS.grey) : null;
    pill.className = !color ? 'pill none' : g.open ? 'pill' : 'pill closed';
    if (color) pill.style.setProperty('--group', color);
    if (!g.open) pill.title = `Closed, last seen ${new Date(g.lastSeen).toLocaleDateString()}`;
    pill.textContent = g.title + ' ';
    const count = document.createElement('span');
    count.className = 'count';
    count.textContent = g.tabs.length;
    pill.append(count);
    if (!g.open) {
      const remove = document.createElement('button');
      remove.className = 'remove';
      remove.textContent = '×';
      remove.title = 'Remove from this list';
      remove.setAttribute('aria-label', `Remove ${g.title}`);
      remove.addEventListener('click', async () => {
        await forgetGroup(g.key);
        await refresh();
        flash(`Removed ${g.title}`);
      });
      pill.append(remove);
    }
    box.append(pill);
  }
}

function plural(n, word) {
  return `${n} ${word}${n === 1 ? '' : 's'}`;
}

let refreshToken = 0;

async function refresh() {
  const token = ++refreshToken;
  const options = {
    currentWindowOnly: $('currentWindow').checked,
    includeUngrouped: $('includeUngrouped').checked,
    includeClosed: $('includeClosed').checked,
    includeDurations: $('includeDurations').checked
  };
  const groups = await collectGroups(options);
  if (token !== refreshToken) return;

  if (options.includeDurations && groups.some((g) => g.tabs.some((t) => videoId(t.url)))) {
    // Show the list straight away, then fill in times once they are fetched.
    $('summary').textContent = 'Fetching video times…';
    await attachDurations(groups);
    if (token !== refreshToken) return;
  }
  state.groups = groups;

  const open = state.groups.filter((g) => g.open && g.color).length;
  const closed = state.groups.filter((g) => !g.open).length;
  const tabCount = state.groups.reduce((n, g) => n + g.tabs.length, 0);
  const times = state.groups.map(groupTime);
  const totalSeconds = times.reduce((n, t) => n + t.seconds, 0);
  const partial = times.some((t) => t.partial);

  $('summary').textContent = state.groups.length === 0
    ? 'No groups yet. Open a saved group once and it will be remembered here, even after you close it.'
    : `${open} open, ${closed} closed, ${plural(tabCount, 'tab')}`
      + (totalSeconds ? `, ${formatDuration(totalSeconds)}${partial ? '+' : ''} of video` : '');

  renderPills(state.groups);
  $('output').value = formatters[$('format').value](state.groups);
  $('copy').disabled = $('share').disabled = $('saveMd').disabled = $('tasks').disabled = state.groups.length === 0;
  $('forget').hidden = closed === 0;

  try {
    localStorage.setItem('tge-options', JSON.stringify({ ...options, format: $('format').value }));
  } catch { /* storage unavailable */ }
}

function flash(message) {
  const el = !$('sharePanel').hidden ? $('shareStatus')
    : !$('tasksPanel').hidden ? $('tasksStatus')
    : $('status');
  el.textContent = message;
  clearTimeout(flash.timer);
  flash.timer = setTimeout(() => (el.textContent = ''), 4000);
}

// ---------- 4. Copy ----------

async function copyList() {
  try {
    await navigator.clipboard.writeText($('output').value);
  } catch {
    $('output').select();
    document.execCommand('copy');
  }
  flash('Copied');
}

// Chrome only lets extensions save inside Downloads, or wherever the user picks
// in Save As. Save As remembers the last folder, so after choosing Desktop once
// it keeps opening there. A data: URL is used because a blob URL would be
// revoked when the popup closes behind the dialog.
const MD_FILENAME = 'YT-List.md';

async function saveMarkdown() {
  const url = 'data:text/markdown;charset=utf-8,' + encodeURIComponent(formatters.markdown(state.groups));
  try {
    await chrome.downloads.download({ url, filename: MD_FILENAME, saveAs: true, conflictAction: 'overwrite' });
    flash('Saved');
  } catch {
    flash('Could not save the file');
  }
}

// ---------- 5. Share ----------

const SHARE_TITLE = 'My tab groups';
// Practical limits for the encoded text a mailto: / wa.me link will carry.
const MAILTO_LIMIT = 1800;
const WHATSAPP_LIMIT = 3500;

function openShare() {
  const scope = $('shareScope');
  scope.replaceChildren(new Option(`All groups (${plural(state.groups.length, 'group')})`, 'all'));
  state.groups.forEach((g, i) => scope.append(new Option(`${g.title} (${g.tabs.length})`, i)));
  $('mainView').hidden = true;
  $('sharePanel').hidden = false;
  renderShare();
}

function closeShare() {
  $('sharePanel').hidden = true;
  $('mainView').hidden = false;
}

function sharedGroups() {
  const v = $('shareScope').value;
  return v === 'all' ? state.groups : [state.groups[Number(v)]];
}

function sharedText() {
  return formatters[$('format').value](sharedGroups());
}

// Past version 20 (97 modules) the squares get too small to scan off a
// popup-sized screen, so larger text is treated as "too long for a QR".
const MAX_QR_MODULES = 97;

// Try medium error correction first and fall back to low only when needed.
function buildQr(text) {
  for (const level of ['M', 'L']) {
    try {
      const qr = qrcode(0, level);
      qr.addData(text);
      qr.make();
      if (qr.getModuleCount() <= MAX_QR_MODULES) return qr;
    } catch { /* too long for this level */ }
  }
  return null;
}

function drawQr(qr) {
  const canvas = $('qr');
  const ctx = canvas.getContext('2d');
  const quiet = 4;
  const count = qr.getModuleCount();
  const cell = canvas.width / (count + quiet * 2);
  ctx.fillStyle = '#fff';
  ctx.fillRect(0, 0, canvas.width, canvas.height);
  ctx.fillStyle = '#000';
  for (let r = 0; r < count; r++) {
    for (let c = 0; c < count; c++) {
      if (qr.isDark(r, c)) {
        const x = Math.round((c + quiet) * cell);
        const y = Math.round((r + quiet) * cell);
        ctx.fillRect(x, y, Math.round((c + quiet + 1) * cell) - x, Math.round((r + quiet + 1) * cell) - y);
      }
    }
  }
}

function renderShare() {
  const groups = sharedGroups();
  const tabCount = groups.reduce((n, g) => n + g.tabs.length, 0);
  // The QR carries titles + URLs only; it is the smallest readable form.
  const qr = buildQr(formatters.urls(groups));
  $('qr').hidden = !qr;
  if (qr) {
    drawQr(qr);
    $('qrNote').textContent = `Scan with your phone camera to get ${plural(tabCount, 'link')}.`;
  } else {
    $('qrNote').textContent = 'Too many links for one QR code. Pick a single group above, or use Email / WhatsApp.';
  }

  // The popup closes when the mail or WhatsApp tab opens, so warn up front.
  const long = encodeURIComponent(sharedText()).length > MAILTO_LIMIT;
  $('shareStatus').textContent = long
    ? 'Long list: Email will be trimmed, and the full list is copied so you can paste it.'
    : '';
}

// Trims to whole lines so a link is never cut in half; returns the text and
// whether anything was dropped.
function fitText(text, limit) {
  if (encodeURIComponent(text).length <= limit) return { text, trimmed: false };
  const lines = text.split('\n');
  while (lines.length > 1 && encodeURIComponent(lines.join('\n') + '\n…').length > limit) lines.pop();
  return { text: lines.join('\n') + '\n…', trimmed: true };
}

async function sendVia(buildUrl, limit) {
  const full = sharedText();
  const { text, trimmed } = fitText(full, limit);
  if (trimmed) {
    try { await navigator.clipboard.writeText(full); } catch { /* clipboard blocked */ }
  }
  await chrome.tabs.create({ url: buildUrl(text) });
  if (trimmed) flash('List was long, so it was trimmed. Full list is copied: paste it to finish.');
}

function shareEmail() {
  const groups = sharedGroups();
  const subject = groups.length === 1 ? groups[0].title : SHARE_TITLE;
  return sendVia(
    (body) => `mailto:?subject=${encodeURIComponent(subject)}&body=${encodeURIComponent(body)}`,
    MAILTO_LIMIT
  );
}

function shareWhatsapp() {
  return sendVia((text) => `https://wa.me/?text=${encodeURIComponent(text)}`, WHATSAPP_LIMIT);
}

// System share sheet where Chrome offers one, otherwise a file download.
async function shareMore() {
  const text = sharedText();

  if (navigator.share && navigator.canShare?.({ title: SHARE_TITLE, text })) {
    try {
      await navigator.share({ title: SHARE_TITLE, text });
      flash('Shared');
      return;
    } catch (err) {
      if (err.name === 'AbortError') return;
    }
  }

  const ext = { markdown: 'md', json: 'json' }[$('format').value] || 'txt';
  const blob = new Blob([text], { type: 'text/plain' });
  const a = Object.assign(document.createElement('a'), {
    href: URL.createObjectURL(blob),
    download: `tab-groups-${new Date().toISOString().slice(0, 10)}.${ext}`
  });
  a.click();
  URL.revokeObjectURL(a.href);
  flash('Saved as file');
}

// ---------- 6. Add to Tasks ----------

function setTasksStatus(message) {
  clearTimeout(flash.timer);
  $('tasksStatus').textContent = message;
}

async function openTasks() {
  const scope = $('tasksScope');
  scope.replaceChildren(new Option(`All groups (${plural(state.groups.length, 'group')})`, 'all'));
  state.groups.forEach((g, i) => scope.append(new Option(`${g.title} (${g.tabs.length})`, i)));
  $('tasksStatus').textContent = '';
  $('mainView').hidden = true;
  $('tasksPanel').hidden = false;
  await renderTasks();
}

function closeTasks() {
  $('tasksPanel').hidden = true;
  $('mainView').hidden = false;
}

async function renderTasks() {
  const config = await loadConfig();
  const signedIn = !!config.token;
  $('tasksSignin').hidden = signedIn;
  $('tasksReady').hidden = !signedIn;
  if (signedIn) {
    $('tasksWho').textContent = config.username || 'you';
  } else {
    if (!$('tasksServer').value) $('tasksServer').value = config.serverUrl || DEFAULT_SERVER;
    if (config.username && !$('tasksUser').value) $('tasksUser').value = config.username;
  }
}

function tasksGroups() {
  const v = $('tasksScope').value;
  return v === 'all' ? state.groups : [state.groups[Number(v)]];
}

async function tasksSignIn(event) {
  event.preventDefault();
  let serverUrl;
  try {
    serverUrl = normalizeServerUrl($('tasksServer').value);
  } catch (err) {
    setTasksStatus(err.message);
    return;
  }
  const button = $('tasksSignInBtn');
  button.disabled = true;
  try {
    // Must be the first await: Chrome only shows the prompt during the click.
    const granted = await chrome.permissions.request({ origins: [originPattern(serverUrl)] });
    if (!granted) {
      setTasksStatus('Chrome needs access to the server to send tasks.');
      return;
    }
    setTasksStatus('Signing in…');
    await signIn(serverUrl, $('tasksUser').value, $('tasksPass').value);
    $('tasksPass').value = '';
    setTasksStatus('');
    await renderTasks();
  } catch (err) {
    setTasksStatus(err.message);
  } finally {
    button.disabled = false;
  }
}

async function tasksSignOut() {
  await signOut();
  setTasksStatus('Signed out');
  await renderTasks();
}

function describeResult({ added, timed, reopened, ticked, skipped, groups }) {
  const changed = groups.filter((g) => g.added > 0);
  const same = groups.filter((g) => !g.added && !g.timed && !g.reopened && !g.ticked).length;
  const parts = [];
  if (changed.length === 1) parts.push(`Added ${plural(added, 'video')} to ${changed[0].title}`);
  else if (changed.length > 1) parts.push(`Added ${plural(added, 'video')} to ${plural(changed.length, 'group')}`);
  if (ticked) parts.push(`Ticked off ${plural(ticked, 'closed video')}`);
  if (reopened) parts.push(`Reopened ${plural(reopened, 'ticked-off video')}`);
  if (timed) parts.push(`Set the time on ${plural(timed, 'video')}`);
  if (same) parts.push(`${plural(same, 'group')} up to date`);
  if (skipped) parts.push(`${plural(skipped, 'group')} without videos skipped`);
  return parts.join(' · ') || 'Nothing to add';
}

async function tasksAdd() {
  const button = $('tasksAdd');
  button.disabled = true;
  setTasksStatus('Adding…');
  try {
    setTasksStatus(describeResult(await addGroups(tasksGroups())));
  } catch (err) {
    setTasksStatus(err.message);
    if (err.status === 401) await renderTasks();
  } finally {
    button.disabled = false;
  }
}

// ---------- Wire up ----------

function restoreOptions() {
  try {
    const saved = JSON.parse(localStorage.getItem('tge-options') || '{}');
    $('currentWindow').checked = !!saved.currentWindowOnly;
    $('includeUngrouped').checked = !!saved.includeUngrouped;
    $('includeClosed').checked = saved.includeClosed ?? true;
    $('includeDurations').checked = saved.includeDurations ?? true;
    if (saved.format && formatters[saved.format]) $('format').value = saved.format;
  } catch {
    $('includeClosed').checked = true;
    $('includeDurations').checked = true;
  }
}

['currentWindow', 'includeUngrouped', 'includeClosed', 'includeDurations', 'format']
  .forEach((id) => $(id).addEventListener('change', refresh));
$('copy').addEventListener('click', copyList);
$('saveMd').addEventListener('click', saveMarkdown);
$('share').addEventListener('click', openShare);
$('shareBack').addEventListener('click', closeShare);
$('shareScope').addEventListener('change', renderShare);
$('shareEmail').addEventListener('click', shareEmail);
$('shareWhatsapp').addEventListener('click', shareWhatsapp);
$('shareMore').addEventListener('click', shareMore);
$('tasks').addEventListener('click', openTasks);
$('tasksBack').addEventListener('click', closeTasks);
$('tasksScope').addEventListener('change', () => setTasksStatus(''));
$('tasksSignin').addEventListener('submit', tasksSignIn);
$('tasksSignOut').addEventListener('click', tasksSignOut);
$('tasksAdd').addEventListener('click', tasksAdd);
$('forget').addEventListener('click', async () => {
  await forgetClosed();
  await refresh();
  flash('Closed groups forgotten');
});

restoreOptions();
refresh();
