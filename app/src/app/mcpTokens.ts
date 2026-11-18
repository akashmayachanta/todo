// Settings → AI: the account's MCP tokens, which let an AI client use the
// server's MCP endpoint ({base}/mcp, server/src/mcp.rs). A browser calls the
// server itself; the Windows app goes through Rust (commands.rs), which holds
// the device token. Errors read "<status>: <body>" either way, for errorDetail().

import { API_BASE, webMcp } from './webClient';
import { call } from './tauriClient';

export type McpScope = 'read' | 'read_write';

export interface McpToken {
	id: string;
	name: string;
	scope: McpScope;
	created_at: string;
	/** null: never expires. */
	expires_at: string | null;
	last_used_at: string | null;
}

export interface NewMcpToken {
	/** Shown once; the server keeps only its hash. */
	token: string;
	info: McpToken;
}

export interface McpApi {
	/** The address an MCP client connects to. */
	url(): Promise<string>;
	list(): Promise<McpToken[]>;
	/** `expiresAt`: an ISO instant, or null for never. */
	create(name: string, scope: McpScope, expiresAt: string | null): Promise<NewMcpToken>;
	setExpiry(id: string, expiresAt: string | null): Promise<McpToken>;
	delete(id: string): Promise<void>;
}

/** API_BASE is a path on this origin, or a whole URL in development. */
function webUrl(): string {
	const base = /^https?:\/\//.test(API_BASE) ? API_BASE : `${location.origin}${API_BASE}`;
	return `${base.replace(/\/+$/, '')}/mcp`;
}

const web: McpApi = {
	url: async () => webUrl(),
	list: () => webMcp.list<McpToken[]>(),
	create: (name, scope, expiresAt) => webMcp.create<NewMcpToken>({ name, scope, expires_at: expiresAt }),
	setExpiry: (id, expiresAt) => webMcp.setExpiry<McpToken>(id, expiresAt),
	delete: (id) => webMcp.delete(id),
};

// Tauri hands Rust's snake_case parameters over as camelCase.
const app: McpApi = {
	url: () => call<string>('app_mcp_url'),
	list: () => call<McpToken[]>('app_mcp_tokens'),
	create: (name, scope, expiresAt) => call<NewMcpToken>('app_mcp_token_create', { name, scope, expiresAt }),
	setExpiry: (id, expiresAt) => call<McpToken>('app_mcp_token_expiry', { id, expiresAt }),
	delete: (id) => call('app_mcp_token_delete', { id }).then(() => undefined),
};

export const mcpApi = (platform: string): McpApi => (platform === 'web' ? web : app);

// ── Expiry ───────────────────────────────────────────────────────────────────

/** A day count, a date the user picks, or never. */
export type ExpiryChoice = '7' | '30' | '90' | 'custom' | 'never';

export const EXPIRY_CHOICES: { value: ExpiryChoice; label: string }[] = [
	{ value: '7', label: '7 days' },
	{ value: '30', label: '30 days' },
	{ value: '90', label: '90 days' },
	{ value: 'custom', label: 'Pick a date' },
	{ value: 'never', label: 'No expiry' },
];

export const DEFAULT_EXPIRY: ExpiryChoice = '30';

const pad = (n: number) => String(n).padStart(2, '0');

/** A local date as YYYY-MM-DD, for an <input type="date">. */
export function dayString(d: Date): string {
	return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/** The first day a custom expiry can be: tomorrow. */
export function firstExpiryDay(now = new Date()): string {
	return dayString(new Date(now.getFullYear(), now.getMonth(), now.getDate() + 1));
}

/**
 * The instant to send for a choice: N days from now, the end of the picked
 * day in local time, or null for never. Throws when a custom date is missing
 * or not in the future.
 */
export function expiryInstant(choice: ExpiryChoice, day: string, now = new Date()): string | null {
	if (choice === 'never') return null;
	if (choice === 'custom') {
		const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day);
		if (!m) throw new Error('Pick the day the token stops working.');
		const end = new Date(Number(m[1]), Number(m[2]) - 1, Number(m[3]), 23, 59, 59, 999);
		if (end.getTime() <= now.getTime()) throw new Error('Pick a day after today.');
		return end.toISOString();
	}
	return new Date(now.getTime() + Number(choice) * 86_400_000).toISOString();
}

/** "12 Dec 2026". */
export function shortDate(iso: string): string {
	return new Date(iso).toLocaleDateString(undefined, { day: 'numeric', month: 'short', year: 'numeric' });
}

/** What the list says about a token's expiry. */
export function expiryText(token: McpToken, now = Date.now()): { text: string; expired: boolean } {
	if (!token.expires_at) return { text: 'Never expires', expired: false };
	const expired = new Date(token.expires_at).getTime() <= now;
	return { text: `${expired ? 'Expired' : 'Expires'} ${shortDate(token.expires_at)}`, expired };
}

// ── Setup text ───────────────────────────────────────────────────────────────

export function claudeCodeCommand(url: string, token: string): string {
	return `claude mcp add --transport http tasks ${url} --header "Authorization: Bearer ${token}"`;
}

export function mcpJson(url: string, token: string): string {
	return JSON.stringify({ mcpServers: { tasks: { type: 'http', url, headers: { Authorization: `Bearer ${token}` } } } }, null, 2);
}

/** Copies text; falls back to a hidden textarea where the Clipboard API isn't
 *  available (a browser on plain http). */
export async function copyText(text: string): Promise<void> {
	if (navigator.clipboard && window.isSecureContext) {
		await navigator.clipboard.writeText(text);
		return;
	}
	const area = document.createElement('textarea');
	area.value = text;
	area.setAttribute('readonly', '');
	area.style.position = 'fixed';
	area.style.opacity = '0';
	document.body.appendChild(area);
	area.select();
	const ok = document.execCommand('copy');
	area.remove();
	if (!ok) throw new Error("Couldn't copy. Select the text and copy it yourself.");
}

/** True when the server couldn't be reached at all. */
export function unreachable(err: unknown): boolean {
	const message = err instanceof Error ? err.message : String(err);
	return /^(0|502|503|504): /.test(message);
}
