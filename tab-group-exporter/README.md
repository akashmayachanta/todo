# Tab Group Exporter

A small Chrome extension that turns your browser's tab groups into one list you
can copy, save, or share — including groups you've already closed.

![version](https://img.shields.io/badge/version-1.6.0-blue)

## Why

Chrome lets you save tab groups, but those saved groups are invisible to
extensions — and once a group is closed, there's no built-in way to get its tabs
back as a list. This extension quietly remembers every group it sees, so you can
export open **and** closed groups whenever you want.

## Features

- **Open + closed groups** — closed groups keep their last known tab list.
- **Four formats** — Markdown (links), plain text, URLs only, or JSON.
- **Video times** — for YouTube links, it shows each video's length and a group
  total, so you can see how long a watch list will take. Totals are marked with
  `+` when some lengths couldn't be found.
- **Filters** — this window only, include ungrouped tabs, include closed groups.
- **Copy & save** — copy the whole list, or save it as `YT-List.md`.
- **Share** — a scannable QR code, Email, WhatsApp, or your system share sheet.
- **Light & dark** — follows your system theme.

## Install (unpacked)

This extension isn't on the Chrome Web Store; you load it directly.

1. Open `chrome://extensions` in Chrome.
2. Turn on **Developer mode** (top right).
3. Click **Load unpacked** and select this folder.
4. Pin **Tab Group Exporter** to your toolbar.

To update after pulling changes, click the **Reload** button on the extension's
card in `chrome://extensions`.

## Use

1. Click the extension icon.
2. Untick anything you don't want (e.g. "Include closed groups").
3. Pick a **Format**.
4. **Tasks**, **Copy list**, **Save .md**, or **Share**.

Closed groups appear as dashed outline pills; the `×` on one removes it from your
list (this can't be undone). **Forget closed** clears all closed groups at once.

## Permissions and privacy

Everything stays on your machine. The extension has no server and sends nothing
anywhere.

| Permission | Why it's needed |
| --- | --- |
| `tabs` | Read your tabs so it can list them. |
| `tabGroups` | Read your groups (titles, colors, membership). |
| `storage` | Keep the local archive of groups and cached video times. |
| `downloads` | Save the `.md` file. |
| `youtube.com` | Look up video lengths from the public watch page. |

The only network request is fetching a YouTube video's public watch page to read
its length; lengths are cached so each video is fetched once.

## Notes

- A group is identified by its **title and color**. Renaming or recoloring a group
  after closing it makes it look like a new group.
- Long lists may be trimmed when sent through Email or WhatsApp (link limits), but
  the full list is copied to your clipboard so you can paste it.
- Very long lists won't fit in a single QR code; share a single group instead.

## Files

| File | What it is |
| --- | --- |
| `manifest.json` | Extension configuration. |
| `background.js` | Watches tab/group events and saves snapshots. |
| `archive.js` | Stores and updates the group archive. |
| `durations.js` | Fetches and caches YouTube video lengths. |
| `popup.html` / `popup.js` | The popup you see when you click the icon. |
| `vendor/qrcode.js` | QR code generator (MIT, qrcode-generator 1.4.4). |
| `YT-List.md` | A sample export. |
| `YT-List.html` | A standalone viewer for a sample export. |

If you're contributing to the code, see [`AGENTS.md`](AGENTS.md).
