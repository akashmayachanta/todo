// Links in task text: `[label](https://…)` shows as the label, and a bare
// http(s) address shows as itself. The text is stored as typed.

export interface TextPart {
	text: string;
	/** Set on a link. */
	url?: string;
}

const LABEL = String.raw`((?:[^\[\]\n]|\[[^\[\]\n]*\])+)`;
const TARGET = String.raw`(https?:\/\/(?:[^\s()]|\([^\s()]*\))+)`;
const BARE = String.raw`(https?:\/\/[^\s<>\[\]]+)`;
const linkPattern = () => new RegExp(String.raw`\[${LABEL}\]\(${TARGET}\)|${BARE}`, 'gi');

/** A bare address without the punctuation that ends the sentence around it. */
function trimBare(url: string): string {
	const trimmed = url.replace(/[.,;:!?'"]+$/, '');
	return trimmed.endsWith(')') && !trimmed.includes('(') ? trimmed.slice(0, -1) : trimmed;
}

/** The text split into plain runs and links. */
export function linkParts(text: string): TextPart[] {
	const parts: TextPart[] = [];
	let at = 0;
	for (const m of text.matchAll(linkPattern())) {
		const whole = m[3] ? trimBare(m[3]) : m[0];
		if (m.index > at) parts.push({ text: text.slice(at, m.index) });
		parts.push(m[3] ? { text: whole, url: whole } : { text: m[1], url: m[2] });
		at = m.index + whole.length;
	}
	if (at < text.length) parts.push({ text: text.slice(at) });
	return parts;
}

/** The text as read, for places that can't hold a link: "[Talk](https://…)" is "Talk". */
export function plainText(text: string): string {
	return linkParts(text).map((p) => p.text).join('');
}

/** The text with its links blanked out, so offsets stay the same. Smart parsing skips them. */
export function blankLinks(text: string): string {
	return text.replace(linkPattern(), (m) => ' '.repeat(m.length));
}
