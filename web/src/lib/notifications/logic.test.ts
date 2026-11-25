import { describe, expect, it, vi } from 'vitest';
import {
	actorName,
	badgeText,
	bellLabel,
	createUnreadPoller,
	markedRead,
	mergeNotifications,
	notificationHref,
	notificationText,
	openRequest,
	withoutOpenRequest,
	type Notification
} from '#lib/notifications/logic.ts';

const base: Notification = {
	id: 'n1',
	kind: 'share',
	actor: { username: 'alice', display_name: 'Alice' },
	subject: { kind: 'folder', id: 'f1', name: 'Trip', mime: null, folder_id: null },
	permission: 'write',
	excerpt: null,
	comment_id: null,
	created_at: '2026-11-24T10:00:00Z',
	read: false
};
const comment = (subject: Partial<Notification['subject']>, excerpt: string | null = 'Nice!'): Notification => ({
	...base,
	kind: 'comment',
	permission: null,
	excerpt,
	comment_id: 'c1',
	subject: { ...base.subject, ...subject }
});
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe('notificationText', () => {
	it('words shares with their permission', () => {
		expect(notificationText(base)).toBe('shared “Trip” with you · can edit');
		expect(notificationText({ ...base, permission: 'read' })).toBe('shared “Trip” with you · can view');
	});

	it('words comments with their excerpt, if any', () => {
		expect(notificationText(comment({ kind: 'file', name: 'beach.jpg' }))).toBe('commented on “beach.jpg”: Nice!');
		expect(notificationText(comment({ name: 'Trip' }, null))).toBe('commented on “Trip”');
	});

	it('names deleted accounts', () => {
		expect(actorName(base)).toBe('Alice');
		expect(actorName({ ...base, actor: null })).toBe('Deleted user');
	});
});

describe('notificationHref', () => {
	it('opens folders and albums themselves', () => {
		expect(notificationHref(base)).toBe('/files/f1');
		expect(notificationHref({ ...base, subject: { ...base.subject, kind: 'album', id: 'a1' } })).toBe('/photos/albums/a1');
	});

	it('opens a file in its folder, else from the Shared page', () => {
		const file = { ...base.subject, kind: 'file' as const, id: 'x1', folder_id: 'f9' };
		expect(notificationHref({ ...base, subject: file })).toBe('/files/f9?open=x1');
		expect(notificationHref({ ...base, subject: { ...file, folder_id: null } })).toBe('/shared?open=x1');
	});

	it('opens the comment thread for comments', () => {
		expect(notificationHref(comment({}))).toBe('/files/f1?comments=1');
		expect(notificationHref(comment({ kind: 'album', id: 'a1' }))).toBe('/photos/albums/a1?comments=1');
		expect(notificationHref(comment({ kind: 'file', id: 'x1', folder_id: 'f9' }))).toBe('/files/f9?open=x1&comments=1');
	});
});

describe('openRequest', () => {
	it('reads and clears the request', () => {
		const url = new URL('http://h/files/f9?open=x1&comments=1&q=a');
		expect(openRequest(url)).toEqual({ open: 'x1', comments: true });
		expect(withoutOpenRequest(url).href).toBe('http://h/files/f9?q=a');
		expect(openRequest(new URL('http://h/files/f1?comments=1'))).toEqual({ open: null, comments: true });
		expect(openRequest(new URL('http://h/files/f1'))).toBeNull();
	});
});

describe('badge', () => {
	it('caps the count', () => {
		expect(badgeText(0)).toBe('');
		expect(badgeText(7)).toBe('7');
		expect(badgeText(120)).toBe('99+');
		expect(bellLabel(0)).toBe('Notifications');
		expect(bellLabel(3)).toBe('Notifications, 3 unread');
	});
});

describe('list updates', () => {
	it('appends pages without duplicates', () => {
		const b = { ...base, id: 'n2' };
		expect(mergeNotifications([base], [base, b]).map((n) => n.id)).toEqual(['n1', 'n2']);
	});

	it('marks some or all read without touching the input', () => {
		const list = [base, { ...base, id: 'n2' }];
		expect(markedRead(list, ['n2']).map((n) => n.read)).toEqual([false, true]);
		expect(markedRead(list).every((n) => n.read)).toBe(true);
		expect(list.every((n) => !n.read)).toBe(true);
	});
});

describe('createUnreadPoller', () => {
	function harness(options: { visible?: () => boolean; answers?: number[]; fail?: boolean } = {}) {
		const timers: (() => void)[] = [];
		const answers = [...(options.answers ?? [1, 2, 3])];
		const onCount = vi.fn();
		const fetch = vi.fn(async () =>
			options.fail
				? { ok: false as const, error: 'down', status: 0 }
				: { ok: true as const, data: { unread: answers.shift() ?? 0 } }
		);
		const poller = createUnreadPoller({
			fetch,
			onCount,
			visible: options.visible,
			schedule: (run) => {
				timers.push(run);
				return () => {
					const index = timers.indexOf(run);
					if (index >= 0) timers.splice(index, 1);
				};
			}
		});
		return { poller, timers, onCount, fetch };
	}

	it('asks now, then on every interval', async () => {
		const h = harness();
		h.poller.refresh();
		await flush();
		expect(h.onCount).toHaveBeenLastCalledWith(1);
		expect(h.timers).toHaveLength(1);
		h.timers.shift()?.();
		await flush();
		expect(h.onCount).toHaveBeenLastCalledWith(2);
		expect(h.timers).toHaveLength(1);
	});

	it('skips its turn while the tab is hidden, and keeps one timer', async () => {
		let shown = false;
		const h = harness({ visible: () => shown });
		h.poller.refresh();
		await flush();
		expect(h.fetch).not.toHaveBeenCalled();
		expect(h.timers).toHaveLength(1);
		shown = true;
		h.poller.refresh();
		await flush();
		expect(h.fetch).toHaveBeenCalledOnce();
		expect(h.timers).toHaveLength(1);
	});

	it('drops a refresh while a request is in flight', async () => {
		const h = harness();
		h.poller.refresh();
		h.poller.refresh();
		await flush();
		expect(h.fetch).toHaveBeenCalledOnce();
		expect(h.timers).toHaveLength(1);
	});

	it('keeps the last count on failure and stops for good', async () => {
		const h = harness({ fail: true });
		h.poller.refresh();
		await flush();
		expect(h.onCount).not.toHaveBeenCalled();
		expect(h.timers).toHaveLength(1);
		h.poller.stop();
		expect(h.timers).toHaveLength(0);
		h.poller.refresh();
		await flush();
		expect(h.fetch).toHaveBeenCalledOnce();
	});
});
