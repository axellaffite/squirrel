/** The shared text editor's states, wording and presence (no I/O). */

/** Server close codes of the collaboration socket. */
export const CLOSE_SIGNED_OUT = 4401;
export const CLOSE_NO_ACCESS = 4404;
export const CLOSE_STALE_EPOCH = 4409;
export const CLOSE_TOO_LARGE = 4413;
export const CLOSE_BUSY = 4429;
export const CLOSE_STORAGE_FULL = 4507;

export type EndReason = 'signed-out' | 'no-access' | 'too-large' | 'busy' | 'storage-full' | 'refused';

/** What a closed socket means for the editor. */
export type CloseOutcome =
	/** A network drop: the provider keeps reconnecting. */
	| { kind: 'reconnect' }
	/** The file changed outside the editor: start over with a fresh document and epoch. */
	| { kind: 'reopen' }
	/** Reconnecting cannot help: stop and explain. */
	| { kind: 'end'; reason: EndReason };

export function closeOutcome(code: number): CloseOutcome {
	switch (code) {
		case CLOSE_SIGNED_OUT:
			return { kind: 'end', reason: 'signed-out' };
		case CLOSE_NO_ACCESS:
			return { kind: 'end', reason: 'no-access' };
		case CLOSE_STALE_EPOCH:
			return { kind: 'reopen' };
		case CLOSE_TOO_LARGE:
			return { kind: 'end', reason: 'too-large' };
		case CLOSE_BUSY:
			return { kind: 'end', reason: 'busy' };
		case CLOSE_STORAGE_FULL:
			return { kind: 'end', reason: 'storage-full' };
		default:
			// Any other 4400–4499 code is a deliberate refusal (y-websocket's convention).
			return code >= 4400 && code < 4500 ? { kind: 'end', reason: 'refused' } : { kind: 'reconnect' };
	}
}

/** Whether the provider may reconnect by itself after the server closed with `code`. */
export const shouldReconnect = (code: number): boolean => closeOutcome(code).kind === 'reconnect';

const END_MESSAGES: Record<EndReason, string> = {
	'signed-out': 'You were signed out. Log in again to keep editing.',
	'no-access': 'This file is no longer available to you: it was deleted, moved to the trash, or unshared.',
	'too-large': 'The document reached the 1 MiB limit. Your last change was not kept.',
	busy: 'The server is busy. Try again in a moment.',
	'storage-full': "The owner's storage is full: your latest changes could not be saved. Free some space, then open the file again.",
	refused: 'The server closed the editor. Try again in a moment.'
};

export const endMessage = (reason: EndReason): string => END_MESSAGES[reason];

/** The answer to opening a file for editing (`POST /api/files/{id}/collab`), as the page sees it. */
export type OpenProblem = 'not-found' | 'not-editable' | 'signed-out' | 'busy' | 'failed';

export function openProblem(status: number, code: string | null): OpenProblem {
	if (status === 401) return 'signed-out';
	if (status === 404 || status === 403) return 'not-found';
	if (status === 422 && code === 'not_editable') return 'not-editable';
	if (status === 429 || status === 503) return 'busy';
	return 'failed';
}

const OPEN_MESSAGES: Record<OpenProblem, string> = {
	'signed-out': END_MESSAGES['signed-out'],
	'not-found': 'This file does not exist, was deleted, or is not shared with you.',
	'not-editable': 'This file cannot be edited here: only UTF-8 text files up to 1 MiB can.',
	busy: END_MESSAGES.busy,
	failed: 'Could not open the editor. Check your connection and try again.'
};

export const openMessage = (problem: OpenProblem): string => OPEN_MESSAGES[problem];

/** The server saves 2 s after the last change: "Saved automatically" shows once a bit longer passed. */
export const SAVE_QUIET_MS = 3000;

export type Phase = 'connecting' | 'connected' | 'reconnecting' | 'ended';

export interface StatusInput {
	phase: Phase;
	/** Some change reached the document since it connected. */
	changed: boolean;
	/** A change happened in the last few seconds (the server has not saved it yet). */
	pending: boolean;
}

/** The quiet status line next to the file name. */
export function statusLabel({ phase, changed, pending }: StatusInput): string {
	if (phase === 'connecting') return 'Connecting…';
	if (phase === 'reconnecting') return 'Reconnecting…';
	if (phase === 'ended') return 'Disconnected';
	if (changed && !pending) return 'Saved automatically';
	return 'Connected';
}

/**
 * Presence colours: design tokens only. Remote peers send one of these strings; anything else
 * is replaced before it reaches the page (it lands in a `style` attribute).
 */
export const PRESENCE_TOKENS = ['--accent', '--success', '--warning', '--ink-muted'] as const;
export const PRESENCE_COLORS: readonly string[] = PRESENCE_TOKENS.map((token) => `var(${token})`);

/** The tinted background of a peer's selection, derived from its colour. */
export const selectionTint = (color: string): string => `color-mix(in srgb, ${color} 22%, transparent)`;

/** A stable colour per account (FNV-1a over its id). */
export function presenceColor(userId: string): string {
	let hash = 0x811c9dc5;
	for (let i = 0; i < userId.length; i += 1) {
		hash ^= userId.charCodeAt(i);
		hash = Math.imul(hash, 0x01000193);
	}
	return PRESENCE_COLORS[(hash >>> 0) % PRESENCE_COLORS.length];
}

/** What each client publishes as `user` in its awareness state. */
export interface PresenceUser {
	id: string;
	name: string;
	color: string;
	colorLight: string;
}

export const NAME_MAX = 64;

export function presenceUser(id: string, name: string): PresenceUser {
	const color = presenceColor(id);
	return { id, name, color, colorLight: selectionTint(color) };
}

/** A remote awareness `user`, made safe to render: known colours and a bounded plain name. */
export function sanitizeUser(value: unknown): PresenceUser {
	const raw = typeof value === 'object' && value !== null ? (value as Record<string, unknown>) : {};
	const id = typeof raw.id === 'string' ? raw.id.slice(0, NAME_MAX) : '';
	const name = typeof raw.name === 'string' && raw.name.trim() !== '' ? raw.name.trim().slice(0, NAME_MAX) : 'Someone';
	const color = typeof raw.color === 'string' && PRESENCE_COLORS.includes(raw.color) ? raw.color : presenceColor(id || name);
	return { id, name, color, colorLight: selectionTint(color) };
}

export interface Person {
	key: string;
	name: string;
	initials: string;
	color: string;
	/** This browser tab. */
	self: boolean;
}

export function initials(name: string): string {
	const words = name.trim().split(/\s+/).filter(Boolean);
	if (words.length === 0) return '?';
	const letters = words.length === 1 ? [...words[0]].slice(0, 2) : [[...words[0]][0], [...words[words.length - 1]][0]];
	return letters.join('').toUpperCase();
}

/**
 * The people in the document, one entry per account (several tabs count once), oneself first,
 * then by name. `states` is the awareness map: client id → state.
 */
export function presence(states: ReadonlyMap<number, Record<string, unknown>>, selfClientId: number): Person[] {
	const people = new Map<string, Person>();
	for (const [clientId, state] of states) {
		if (!state || !('user' in state)) continue;
		const user = sanitizeUser(state.user);
		const key = user.id || `client-${clientId}`;
		const self = clientId === selfClientId;
		const existing = people.get(key);
		if (existing && !self) continue;
		people.set(key, { key, name: user.name, initials: initials(user.name), color: user.color, self: self || (existing?.self ?? false) });
	}
	return [...people.values()].sort((a, b) => Number(b.self) - Number(a.self) || a.name.localeCompare(b.name));
}
