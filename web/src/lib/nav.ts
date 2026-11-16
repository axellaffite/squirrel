import type { IconName } from '#lib/components/icons/paths.ts';

export interface NavItem {
	href: string;
	label: string;
	icon: IconName;
}

/** The rail's items, in order. Invitations only for users allowed to invite. */
export function navItems(canInvite: boolean): NavItem[] {
	return [
		{ href: '/', label: 'Home', icon: 'home' },
		{ href: '/photos', label: 'Photos', icon: 'images' },
		{ href: '/files', label: 'Files', icon: 'folder' },
		{ href: '/trash', label: 'Trash', icon: 'trash' },
		...(canInvite ? [{ href: '/settings/invitations', label: 'Invitations', icon: 'user-plus' as const }] : [])
	];
}

/** Whether `href` is the section `path` belongs to (Home only on `/`). */
export function isActive(href: string, path: string): boolean {
	return href === '/' ? path === '/' : path === href || path.startsWith(`${href}/`);
}
