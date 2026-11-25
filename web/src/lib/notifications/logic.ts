/** Wording, links and unread polling for the notification inbox (no I/O of its own). */
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';
import { memberName } from '#lib/sharing/logic.ts';

export type Notification = components['schemas']['NotificationDto'];
export type NotificationPage = components['schemas']['NotificationPage'];

/** How often the unread count is asked while the tab is visible. */
export const UNREAD_POLL_MS = 60_000;
/** Notifications fetched per page of the panel. */
export const NOTIFICATION_PAGE = 30;
/** Badges show at most this many, then "99+". */
const BADGE_MAX = 99;

/** Query parameters a page reads once to open something on arrival. */
export const OPEN_PARAM = 'open';
export const COMMENTS_PARAM = 'comments';

export const actorName = (notification: Notification): string => memberName(notification.actor);

/** What happened, after the actor's name: "shared “Trip” with you · can edit". */
export function notificationText(notification: Notification): string {
	const name = `“${notification.subject.name}”`;
	if (notification.kind === 'share') {
		const can = notification.permission === 'write' ? 'can edit' : 'can view';
		return `shared ${name} with you · ${can}`;
	}
	const excerpt = notification.excerpt?.trim();
	return excerpt ? `commented on ${name}: ${excerpt}` : `commented on ${name}`;
}

/**
 * Where a notification leads, the way the Shared page opens things: a folder or an album
 * itself; a file in its folder when the caller may open it, else from the Shared page. Comment
 * notifications also open the thread.
 */
export function notificationHref(notification: Notification): string {
	const { subject } = notification;
	const params = new URLSearchParams();
	let path: string;
	if (subject.kind === 'folder') path = `/files/${subject.id}`;
	else if (subject.kind === 'album') path = `/photos/albums/${subject.id}`;
	else {
		path = subject.folder_id ? `/files/${subject.folder_id}` : '/shared';
		params.set(OPEN_PARAM, subject.id);
	}
	if (notification.kind === 'comment') params.set(COMMENTS_PARAM, '1');
	const query = params.toString();
	return query ? `${path}?${query}` : path;
}

export interface OpenRequest {
	/** A file to preview once the page loaded. */
	open: string | null;
	/** Open the comment thread (of that file, else of the page's folder or album). */
	comments: boolean;
}

/** A page address (SvelteKit's `page.url` is a read-only URL). */
interface Address {
	readonly href: string;
	readonly searchParams: { get: (name: string) => string | null };
}

/** What `url` asks the page to open, if anything. */
export function openRequest(url: Address): OpenRequest | null {
	const open = url.searchParams.get(OPEN_PARAM);
	const comments = url.searchParams.get(COMMENTS_PARAM) === '1';
	return open || comments ? { open: open || null, comments } : null;
}

/** `url` without the open request, so a reload or "back" does not open it again. */
export function withoutOpenRequest(url: Address): URL {
	const next = new URL(url.href);
	next.searchParams.delete(OPEN_PARAM);
	next.searchParams.delete(COMMENTS_PARAM);
	return next;
}

/** The badge text: nothing at zero, "99+" past 99. */
export const badgeText = (unread: number): string => (unread <= 0 ? '' : unread > BADGE_MAX ? `${BADGE_MAX}+` : String(unread));

/** The bell's accessible name. */
export const bellLabel = (unread: number): string =>
	unread <= 0 ? 'Notifications' : `Notifications, ${unread} unread`;

/** The next page (newest first) after the ones shown. */
export function mergeNotifications(shown: readonly Notification[], page: readonly Notification[]): Notification[] {
	const seen = new Set(shown.map((entry) => entry.id));
	return [...shown, ...page.filter((entry) => !seen.has(entry.id))];
}

export const hasUnread = (list: readonly Notification[] | null): boolean => (list ?? []).some((entry) => !entry.read);

/** The list once `ids` (all when omitted) were marked read. */
export function markedRead(list: readonly Notification[], ids?: readonly string[]): Notification[] {
	const wanted = ids ? new Set(ids) : null;
	return list.map((entry) => (!entry.read && (!wanted || wanted.has(entry.id)) ? { ...entry, read: true } : entry));
}

export interface UnreadPollerOptions {
	fetch: () => Promise<Outcome<{ unread: number }>>;
	onCount: (unread: number) => void;
	/** Whether the tab is visible; hidden tabs skip their turn (no request). */
	visible?: () => boolean;
	schedule?: (run: () => void, ms: number) => () => void;
	intervalMs?: number;
}

export interface UnreadPoller {
	/** Asks now (when visible), then every interval. Also for focus and visibility changes. */
	refresh: () => void;
	stop: () => void;
}

/**
 * Polls the unread count every `intervalMs` while the tab is visible. A refresh while a request
 * is in flight is dropped; failed requests keep the last count.
 */
export function createUnreadPoller(options: UnreadPollerOptions): UnreadPoller {
	const schedule =
		options.schedule ??
		((run: () => void, ms: number) => {
			const handle = setTimeout(run, ms);
			return () => clearTimeout(handle);
		});
	const visible = options.visible ?? (() => true);
	const interval = options.intervalMs ?? UNREAD_POLL_MS;
	let cancel: (() => void) | null = null;
	let running = false;
	let stopped = false;

	function clearTimer() {
		cancel?.();
		cancel = null;
	}

	async function tick() {
		clearTimer();
		if (stopped || running) return;
		if (visible()) {
			running = true;
			const result = await options.fetch();
			running = false;
			if (stopped) return;
			if (result.ok) options.onCount(result.data.unread);
		}
		clearTimer();
		cancel = schedule(() => void tick(), interval);
	}

	return {
		refresh: () => void tick(),
		stop() {
			stopped = true;
			clearTimer();
		}
	};
}
