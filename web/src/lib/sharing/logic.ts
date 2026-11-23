/** Wording and list updates for sharing and comments (no I/O). */
import { sortByName } from '#lib/photos/albums.ts';
import type { Album } from '#lib/photos/api.ts';
import type { Access, Comment, Member, Permission, Share, SharedItem } from '#lib/sharing/api.ts';

/** The server's comment limit, in characters once trimmed. */
export const COMMENT_MAX = 4000;

const MINUTE = 60_000;
const HOUR = 60 * MINUTE;
const DAY = 24 * HOUR;

/** An account's name; comments of deleted accounts keep a `null` author. */
export const memberName = (member: Member | null | undefined): string => member?.display_name ?? 'Deleted user';

export const permissionLabel = (permission: Permission): string => (permission === 'write' ? 'Can edit' : 'Can view');

export interface Abilities {
	/** Share, rename, delete the subject itself. */
	owner: boolean;
	/** Add, rename, move and trash what is inside. */
	edit: boolean;
}

export const abilities = (access: Access): Abilities => ({ owner: access === 'owner', edit: access !== 'read' });

interface WithId {
	id: string;
}

/** A share back from the server: the same id replaces the old one (a permission change). */
export function upsertShare(shares: readonly Share[], share: Share): Share[] {
	return shares.some((s) => s.id === share.id) ? replaceById(shares, share) : [...shares, share];
}

export const replaceById = <T extends WithId>(list: readonly T[], next: T): T[] =>
	list.map((entry) => (entry.id === next.id ? next : entry));

export const removeById = <T extends WithId>(list: readonly T[], id: string): T[] => list.filter((entry) => entry.id !== id);

/** The next page of a thread (oldest first) after the ones shown. */
export function mergeComments(shown: readonly Comment[], page: readonly Comment[]): Comment[] {
	const seen = new Set(shown.map((comment) => comment.id));
	return [...shown, ...page.filter((comment) => !seen.has(comment.id))];
}

export function commentProblem(body: string): string | null {
	const length = [...body.trim()].length;
	if (length === 0) return 'Write a comment first.';
	if (length > COMMENT_MAX) return `Comments have at most ${COMMENT_MAX} characters.`;
	return null;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

/** "Just now", "5 min ago", "3 h ago", then a short date. */
export function commentWhen(iso: string, now: Date = new Date()): string {
	const then = new Date(iso);
	const age = now.getTime() - then.getTime();
	if (age < MINUTE) return 'Just now';
	if (age < HOUR) return `${Math.floor(age / MINUTE)} min ago`;
	if (age < DAY) return `${Math.floor(age / HOUR)} h ago`;
	return dateFormat.format(then);
}

interface Place {
	id: string;
	name: string;
}

/**
 * Where a move may go: nowhere limited for the owner; for others, inside the share only, so
 * browsing starts at the shared folder (the first segment of the path, else the folder itself).
 */
export function moveRoot(access: Access, path: readonly Place[], folder: Place): Place | null {
	if (access === 'owner') return null;
	const top = path[0] ?? folder;
	return { id: top.id, name: top.name };
}

/** "Shared with me", folders and files apart from albums. */
export function splitShared(items: readonly SharedItem[]): { nodes: SharedItem[]; albums: SharedItem[] } {
	return {
		nodes: items.filter((item) => item.node),
		albums: items.filter((item) => item.album)
	};
}

/** The albums photos can be added to: one's own and those shared for writing, by name. */
export function writableAlbums(own: readonly Album[], shared: readonly SharedItem[]): Album[] {
	const ids = new Set(own.map((album) => album.id));
	const extra = shared
		.filter((item) => item.permission === 'write' && item.album && !ids.has(item.album.id))
		.map((item) => item.album as Album);
	return sortByName([...own, ...extra]);
}
