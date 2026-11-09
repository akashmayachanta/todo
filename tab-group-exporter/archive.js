// Keeps a copy of every tab group the extension has seen, so groups stay
// exportable after you close them. Chrome's own saved groups can't be read
// by extensions, so this archive is the only way to list closed groups.

export const GROUP_COLORS = {
  grey: '#5f6368', blue: '#1a73e8', red: '#d93025', yellow: '#e37400',
  green: '#188038', pink: '#d01884', purple: '#9334e6', cyan: '#007b83',
  orange: '#fa903e'
};

const ARCHIVE_KEY = 'groupArchive';

export async function loadArchive() {
  const { [ARCHIVE_KEY]: archive = {} } = await chrome.storage.local.get(ARCHIVE_KEY);
  return archive;
}

async function saveArchive(archive) {
  await chrome.storage.local.set({ [ARCHIVE_KEY]: archive });
}

// Group IDs change every browser session, so groups are matched by title + color.
function keyFor(group) {
  return group.title ? `${group.title}|${group.color}` : `untitled|${group.id}`;
}

// storage.session is cleared when Chrome restarts, giving us a per-session ID.
// It stops a new group from being mistaken for an old one that had the same ID.
async function getSessionId() {
  let { sessionId } = await chrome.storage.session.get('sessionId');
  if (!sessionId) {
    sessionId = crypto.randomUUID();
    await chrome.storage.session.set({ sessionId });
  }
  return sessionId;
}

// Records all open groups and marks everything else as closed.
export async function snapshot() {
  const [groups, tabs, archive, sessionId] = await Promise.all([
    chrome.tabGroups.query({}),
    chrome.tabs.query({}),
    loadArchive(),
    getSessionId()
  ]);

  tabs.sort((a, b) => a.windowId - b.windowId || a.index - b.index);
  const tabsByGroup = new Map();
  for (const tab of tabs) {
    if (tab.groupId === chrome.tabGroups.TAB_GROUP_ID_NONE) continue;
    if (!tabsByGroup.has(tab.groupId)) tabsByGroup.set(tab.groupId, []);
    tabsByGroup.get(tab.groupId).push({
      title: tab.title || tab.url,
      url: tab.url || tab.pendingUrl || ''
    });
  }

  const now = Date.now();
  const liveKeys = new Set();

  for (const group of groups) {
    const groupTabs = tabsByGroup.get(group.id);
    if (!groupTabs?.length) continue;
    const key = keyFor(group);

    // Renamed or recolored this session: drop the entry stored under the old key.
    for (const [oldKey, entry] of Object.entries(archive)) {
      if (oldKey !== key && entry.liveId === group.id && entry.liveSession === sessionId) {
        delete archive[oldKey];
      }
    }

    archive[key] = {
      title: group.title || 'Untitled group',
      color: group.color,
      tabs: groupTabs,
      open: true,
      liveId: group.id,
      liveSession: sessionId,
      firstSeen: archive[key]?.firstSeen ?? now,
      lastSeen: now
    };
    liveKeys.add(key);
  }

  // Anything not open right now is closed; its last known tabs are kept.
  for (const [key, entry] of Object.entries(archive)) {
    if (!liveKeys.has(key)) {
      entry.open = false;
      entry.liveId = null;
    }
  }

  await saveArchive(archive);
  return archive;
}

export async function forgetGroup(key) {
  const archive = await loadArchive();
  if (archive[key] && !archive[key].open) {
    delete archive[key];
    await saveArchive(archive);
  }
}

export async function forgetClosed() {
  const archive = await loadArchive();
  for (const [key, entry] of Object.entries(archive)) {
    if (!entry.open) delete archive[key];
  }
  await saveArchive(archive);
}
