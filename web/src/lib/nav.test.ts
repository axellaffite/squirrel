import { describe, expect, it } from 'vitest';
import { isActive, navItems } from '#lib/nav.ts';

describe('navItems', () => {
	it('puts Photos right after Home', () => {
		expect(navItems(false).map((item) => item.label)).toEqual(['Home', 'Photos', 'Files', 'Trash']);
		expect(navItems(true).map((item) => item.href)).toEqual(['/', '/photos', '/files', '/trash', '/settings/invitations']);
	});

	it('marks the section of the current path', () => {
		expect(isActive('/photos', '/photos/albums')).toBe(true);
		expect(isActive('/photos', '/photosx')).toBe(false);
		expect(isActive('/', '/photos')).toBe(false);
		expect(isActive('/', '/')).toBe(true);
	});
});
