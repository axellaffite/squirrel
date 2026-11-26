import { describe, expect, it } from 'vitest';
import { isActive, navGroups, navItems } from '#lib/nav.ts';

describe('navItems', () => {
	it('puts Photos right after Home, then Search and Explore in the sidebar', () => {
		expect(navItems(false).map((item) => item.label)).toEqual([
			'Home',
			'Photos',
			'Search',
			'Explore',
			'Files',
			'Shared',
			'Trash',
			'Settings'
		]);
		expect(navItems(true).map((item) => item.href)).toEqual([
			'/',
			'/photos',
			'/search',
			'/explore',
			'/files',
			'/shared',
			'/trash',
			'/settings',
			'/settings/invitations'
		]);
	});

	it('keeps the phone bar to five items: Search, Explore, Settings and Invitations are sidebar only', () => {
		const phone = (canInvite: boolean) => navItems(canInvite).filter((item) => !item.sidebarOnly);
		expect(phone(false).map((item) => item.label)).toEqual(['Home', 'Photos', 'Files', 'Shared', 'Trash']);
		expect(phone(true)).toEqual(phone(false));
	});

	it('groups the sidebar into Library, Sharing and a last group for Trash', () => {
		const groups = navGroups(true);
		expect(groups.map((g) => g.label)).toEqual(['Library', 'Sharing', null]);
		expect(groups.map((g) => g.items.map((i) => i.label))).toEqual([
			['Home', 'Photos', 'Search', 'Explore', 'Files'],
			['Shared', 'Invitations'],
			['Trash', 'Settings']
		]);
		expect(navGroups(false)[1].items.map((i) => i.label)).toEqual(['Shared']);
	});

	it('marks the section of the current path', () => {
		expect(isActive('/photos', '/photos/albums')).toBe(true);
		expect(isActive('/photos', '/photosx')).toBe(false);
		expect(isActive('/', '/photos')).toBe(false);
		expect(isActive('/', '/')).toBe(true);
		expect(isActive('/settings', '/settings/appearance')).toBe(true);
		// Invitations has its own item: Settings is not lit there too.
		expect(isActive('/settings', '/settings/invitations')).toBe(false);
		expect(isActive('/settings/invitations', '/settings/invitations')).toBe(true);
	});
});
