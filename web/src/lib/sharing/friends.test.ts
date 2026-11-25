import { describe, expect, it } from 'vitest';
import type { Friend, Member, Share } from '#lib/sharing/api.ts';
import { addToFriends, pickableFriends, withoutPerson } from '#lib/sharing/friends.ts';

function friend(username: string, display_name = username): Friend {
	return { username, display_name, added_at: '2026-11-24T10:00:00Z' };
}

function shareWith(person: Member): Share {
	return { id: person.username, permission: 'read', person, created_at: '2026-11-24T10:00:00Z' };
}

const names = (list: readonly Member[]) => list.map((person) => person.username);

describe('pickableFriends', () => {
	it('leaves out friends who already have access', () => {
		const friends = [friend('bob'), friend('carol'), friend('dave')];
		const shares = [shareWith(friend('carol'))];
		expect(names(pickableFriends(friends, shares))).toEqual(['bob', 'dave']);
	});

	it('offers every friend before the shares are loaded', () => {
		expect(names(pickableFriends([friend('bob')], null))).toEqual(['bob']);
	});
});

describe('addToFriends', () => {
	it('keeps the list sorted by display name, ignoring case, then username', () => {
		const list = [friend('zed', 'Anna'), friend('carol', 'carol')];
		const next = addToFriends(list, friend('bob', 'Bob'));
		expect(names(next)).toEqual(['zed', 'bob', 'carol']);
		expect(names(list)).toEqual(['zed', 'carol']);
	});

	it('replaces a friend added twice', () => {
		const next = addToFriends([friend('bob', 'Bob')], friend('bob', 'Bobby'));
		expect(next).toEqual([friend('bob', 'Bobby')]);
	});
});

describe('withoutPerson', () => {
	it('drops the person by username', () => {
		expect(names(withoutPerson([friend('bob'), friend('carol')], 'bob'))).toEqual(['carol']);
		expect(names(withoutPerson([friend('bob'), friend('carol')], 'carol'))).toEqual(['bob']);
	});
});
