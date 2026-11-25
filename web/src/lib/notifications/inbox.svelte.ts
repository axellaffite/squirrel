/** The unread count, shared by every bell in the shell; one poller for the whole app. */
import { unreadCount } from '#lib/notifications/api.ts';
import { createUnreadPoller } from '#lib/notifications/logic.ts';

export const inbox: { unread: number } = $state({ unread: 0 });

/** Polls while the tab is visible and asks again on focus; returns the teardown. */
export function watchUnread(): () => void {
	const poller = createUnreadPoller({
		fetch: unreadCount,
		onCount: (unread) => (inbox.unread = unread),
		visible: () => document.visibilityState === 'visible'
	});
	const refresh = () => poller.refresh();
	document.addEventListener('visibilitychange', refresh);
	window.addEventListener('focus', refresh);
	refresh();
	return () => {
		poller.stop();
		document.removeEventListener('visibilitychange', refresh);
		window.removeEventListener('focus', refresh);
	};
}

export function setUnread(unread: number): void {
	inbox.unread = unread;
}
