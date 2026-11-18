import { describe, expect, it } from 'vitest';
import { isActive, navItems } from '#lib/nav.ts';

describe('navItems', () => {
	it('puts Photos right after Home, then Search and Explore in the sidebar', () => {
		expect(navItems(false).map((item) => item.label)).toEqual(['Home', 'Photos', 'Search', 'Explore', 'Files', 'Trash']);
		expect(navItems(true).map((item) => item.href)).toEqual([
			'/',
			'/photos',
			'/search',
			'/explore',
			'/files',
			'/trash',
			'/settings/invitations'
		]);
	});

	it('keeps the phone bar to five items: Search and Explore are sidebar only', () => {
		const phone = (canInvite: boolean) => navItems(canInvite).filter((item) => !item.sidebarOnly);
		expect(phone(false).map((item) => item.label)).toEqual(['Home', 'Photos', 'Files', 'Trash']);
		expect(phone(true)).toHaveLength(5);
	});

	it('marks the section of the current path', () => {
		expect(isActive('/photos', '/photos/albums')).toBe(true);
		expect(isActive('/photos', '/photosx')).toBe(false);
		expect(isActive('/', '/photos')).toBe(false);
		expect(isActive('/', '/')).toBe(true);
	});
});
