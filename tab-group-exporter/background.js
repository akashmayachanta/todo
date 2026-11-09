import { snapshot } from './archive.js';

// Wait for bursts of tab events (opening a window, closing a group) to settle
// before saving, so a half-closed group never overwrites its full tab list.
let timer;
function scheduleSnapshot() {
  clearTimeout(timer);
  timer = setTimeout(() => snapshot().catch(console.error), 1500);
}

chrome.runtime.onInstalled.addListener(scheduleSnapshot);
chrome.runtime.onStartup.addListener(scheduleSnapshot);

chrome.tabGroups.onCreated.addListener(scheduleSnapshot);
chrome.tabGroups.onUpdated.addListener(scheduleSnapshot);
chrome.tabGroups.onRemoved.addListener(scheduleSnapshot);

chrome.tabs.onCreated.addListener(scheduleSnapshot);
chrome.tabs.onMoved.addListener(scheduleSnapshot);
chrome.tabs.onAttached.addListener(scheduleSnapshot);
chrome.tabs.onDetached.addListener(scheduleSnapshot);

chrome.tabs.onUpdated.addListener((_tabId, change) => {
  if ('url' in change || 'title' in change || 'groupId' in change) scheduleSnapshot();
});

chrome.tabs.onRemoved.addListener((_tabId, info) => {
  // When a whole window closes, keep the groups' last full snapshot.
  if (!info.isWindowClosing) scheduleSnapshot();
});
