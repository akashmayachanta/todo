// Looks up YouTube video lengths so exports can show time per link.
// YouTube's API needs a key, so this reads the length from the public watch
// page instead. Results are cached by video ID, since lengths never change.

const CACHE_KEY = 'durationCache';
const CONCURRENCY = 6;

const YT_HOSTS = new Set(['www.youtube.com', 'youtube.com', 'm.youtube.com', 'music.youtube.com', 'youtu.be']);

export function videoId(url) {
  let u;
  try { u = new URL(url); } catch { return null; }
  if (!YT_HOSTS.has(u.hostname)) return null;
  const id = u.hostname === 'youtu.be'
    ? u.pathname.slice(1).split('/')[0]
    : u.pathname === '/watch'
      ? u.searchParams.get('v')
      : u.pathname.match(/^\/(?:shorts|embed|live)\/([^/?]+)/)?.[1];
  return id && /^[\w-]{11}$/.test(id) ? id : null;
}

async function fetchSeconds(id) {
  const res = await fetch(`https://www.youtube.com/watch?v=${id}`, { credentials: 'include' });
  if (!res.ok) return null;
  const html = await res.text();
  // lengthSeconds sits right after the title inside videoDetails.
  const start = html.indexOf('"videoDetails"');
  if (start < 0) return null;
  const match = html.slice(start, start + 20000).match(/"lengthSeconds":"(\d+)"/);
  const seconds = match ? Number(match[1]) : 0;
  return seconds > 0 ? seconds : null; // 0 means a live stream
}

// Sets `seconds` on every tab entry that is a YouTube video whose length could
// be found. Entries that fail are left alone, so totals can be flagged partial.
export async function attachDurations(groups) {
  const { [CACHE_KEY]: cache = {} } = await chrome.storage.local.get(CACHE_KEY);

  const wanted = new Map(); // id -> tab entries
  for (const g of groups) {
    for (const tab of g.tabs) {
      const id = videoId(tab.url);
      if (!id) continue;
      tab.video = true;
      if (!wanted.has(id)) wanted.set(id, []);
      wanted.get(id).push(tab);
    }
  }

  const missing = [...wanted.keys()].filter((id) => !(id in cache));
  let dirty = false;
  const queue = [...missing];
  const worker = async () => {
    while (queue.length) {
      const id = queue.shift();
      try {
        const seconds = await fetchSeconds(id);
        if (seconds) { cache[id] = seconds; dirty = true; }
      } catch { /* offline or blocked: leave this one without a time */ }
    }
  };
  await Promise.all(Array.from({ length: Math.min(CONCURRENCY, queue.length) }, worker));
  if (dirty) await chrome.storage.local.set({ [CACHE_KEY]: cache });

  for (const [id, tabs] of wanted) {
    if (cache[id]) tabs.forEach((t) => { t.seconds = cache[id]; });
  }
}

export function formatDuration(total) {
  const h = Math.floor(total / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const ss = String(s).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

// Total for a group, plus whether any video in it is missing a length.
export function groupTime(group) {
  let seconds = 0;
  let partial = false;
  for (const t of group.tabs) {
    if (t.seconds) seconds += t.seconds;
    else if (t.video) partial = true;
  }
  return { seconds, partial };
}
