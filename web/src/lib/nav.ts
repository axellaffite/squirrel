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
}

/** The rail's items, in order. Invitations only for users allowed to invite. */
export function navItems(canInvite: boolean): NavItem[] {
	return [
		{ href: '/', label: 'Home', icon: 'home' },
		{ href: '/photos', label: 'Photos', icon: 'images' },
		{ href: '/search', label: 'Search', icon: 'search', sidebarOnly: true },
		{ href: '/explore', label: 'Explore', icon: 'sparkles', sidebarOnly: true },
		{ href: '/files', label: 'Files', icon: 'folder' },
		{ href: '/shared', label: 'Shared', icon: 'users' },
		{ href: '/trash', label: 'Trash', icon: 'trash' },
		...(canInvite
			? [{ href: '/settings/invitations', label: 'Invitations', icon: 'user-plus' as const, sidebarOnly: true }]
			: [])
	];
}

/** Whether `href` is the section `path` belongs to (Home only on `/`). */
export function isActive(href: string, path: string): boolean {
	return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
}
