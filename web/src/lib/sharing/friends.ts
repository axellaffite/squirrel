/** Friends list updates (no I/O). */
import type { Friend, Member, Share } from '#lib/sharing/api.ts';

/** Friends who do not have access yet, to share with in one tap (all of them while shares load). */
export function pickableFriends(friends: readonly Friend[], shares: readonly Share[] | null): Friend[] {
	const withAccess = new Set((shares ?? []).map((share) => share.person.username));
	return friends.filter((friend) => !withAccess.has(friend.username));
}

const byName = (a: Member, b: Member): number =>
	a.display_name.localeCompare(b.display_name, undefined, { sensitivity: 'base' }) || a.username.localeCompare(b.username);

/** `friend` added (or replaced), in the server's order: display name ignoring case, then username. */
export function addToFriends(friends: readonly Friend[], friend: Friend): Friend[] {
	return [...withoutPerson(friends, friend.username), friend].sort(byName);
}

export const withoutPerson = <T extends Member>(list: readonly T[], username: string): T[] =>
	list.filter((person) => person.username !== username);
