import { api, errorMessage } from '#lib/api/client.ts';
import type { Outcome } from '#lib/files/pages.ts';
import { NOTIFICATION_PAGE, type NotificationPage } from '#lib/notifications/logic.ts';

interface Unread {
	unread: number;
}

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export const listNotifications = (cursor: string | null): Promise<Outcome<NotificationPage>> =>
	call(api.GET('/api/notifications', { params: { query: { cursor: cursor ?? undefined, limit: NOTIFICATION_PAGE } } }));

export const unreadCount = (): Promise<Outcome<Unread>> => call(api.GET('/api/notifications/unread-count'));

/** Marks `ids` read, or every notification when omitted; answers the new unread count. */
export const markRead = (ids?: string[]): Promise<Outcome<Unread>> =>
	call(api.POST('/api/notifications/read', { body: ids ? { ids } : {} }));
