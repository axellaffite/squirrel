import type { IconName } from '#lib/components/icons/paths.ts';

export interface NavItem {
	href: string;
	label: string;
	icon: IconName;
	/**
	 * Sidebar only (from 1024 px). The phone bar keeps at most five items: there, Search is a
	 * round button in the top row, Explore a tab of Photos and Invitations in the account menu.
	 */
	sidebarOnly?: boolean;
	/** The sidebar section the item sits in (MASTER.md › "Sidebar item"). */
	group: NavGroupKey;
}

export type NavGroupKey = 'library' | 'sharing' | 'end';

export interface NavGroup {
	key: NavGroupKey;
	/** Section label shown above the group in the sidebar; the last group has none. */
	label: string | null;
	items: NavItem[];
}

/** Lives under /settings but has its own sidebar item. */
const INVITATIONS = '/settings/invitations';

const GROUP_LABELS: Record<NavGroupKey, string | null> = { library: 'Library', sharing: 'Sharing', end: null };

/** The rail's items, in order. Invitations only for users allowed to invite. */
export function navItems(canInvite: boolean): NavItem[] {
	return [
		{ href: '/', label: 'Home', icon: 'home', group: 'library' },
		{ href: '/photos', label: 'Photos', icon: 'images', group: 'library' },
		{ href: '/search', label: 'Search', icon: 'search', sidebarOnly: true, group: 'library' },
		{ href: '/explore', label: 'Explore', icon: 'sparkles', sidebarOnly: true, group: 'library' },
		{ href: '/files', label: 'Files', icon: 'folder', group: 'library' },
		{ href: '/shared', label: 'Shared', icon: 'users', group: 'sharing' },
		{ href: '/trash', label: 'Trash', icon: 'trash', group: 'end' },
		{ href: '/settings', label: 'Settings', icon: 'sliders-horizontal', sidebarOnly: true, group: 'end' },
		...(canInvite
			? [{ href: '/settings/invitations', label: 'Invitations', icon: 'user-plus' as const, sidebarOnly: true, group: 'sharing' as const }]
			: [])
	];
}

/** The sidebar's sections, in order, each with its items in rail order. Empty groups are dropped. */
export function navGroups(canInvite: boolean): NavGroup[] {
	const items = navItems(canInvite);
	return (['library', 'sharing', 'end'] as const)
		.map((key) => ({ key, label: GROUP_LABELS[key], items: items.filter((item) => item.group === key) }))
		.filter((group) => group.items.length > 0);
}

/** Whether `href` is the section `path` belongs to (Home only on `/`). */
export function isActive(href: string, path: string): boolean {
	if (href === '/settings' && isActive(INVITATIONS, path)) return false;
	return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
}
