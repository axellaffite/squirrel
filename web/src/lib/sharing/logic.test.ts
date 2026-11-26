import { describe, expect, it } from 'vitest';
import type { Album } from '#lib/photos/api.ts';
import type { Comment, Share, SharedItem } from '#lib/sharing/api.ts';
import {
	abilities,
	COMMENT_MAX,
	commentProblem,
	commentWhen,
	memberName,
	mergeComments,
	moveRoot,
	permissionLabel,
	removeById,
	replaceById,
	sharedWithSummary,
	splitShared,
	upsertShare,
	writableAlbums
} from '#lib/sharing/logic.ts';

const bob = { username: 'bob', display_name: 'Bob' };

function share(id: string, permission: 'read' | 'write' = 'read'): Share {
	return { id, permission, person: bob, created_at: '2026-11-23T10:00:00Z' };
}

function comment(id: string, body = 'Hi'): Comment {
	return { id, body, author: bob, created_at: '2026-11-23T10:00:00Z', mine: true, can_delete: true };
}

function album(id: string, name = id, dynamic = false): Album {
	return { id, name, item_count: 0, updated_at: '2026-11-23T10:00:00Z', access: 'owner', dynamic };
}

function item(over: Partial<SharedItem>): SharedItem {
	return { share_id: 's', owner: bob, permission: 'read', shared_at: '2026-11-23T10:00:00Z', ...over };
}

describe('memberName', () => {
	it('names deleted accounts', () => {
		expect(memberName(bob)).toBe('Bob');
		expect(memberName(null)).toBe('Deleted user');
		expect(memberName(undefined)).toBe('Deleted user');
	});
});

describe('permissionLabel', () => {
	it('words both permissions', () => {
		expect(permissionLabel('read')).toBe('Can view');
		expect(permissionLabel('write')).toBe('Can edit');
	});
});

describe('abilities', () => {
	it('lets owners do everything, writers edit, readers look', () => {
		expect(abilities('owner')).toEqual({ owner: true, edit: true });
		expect(abilities('write')).toEqual({ owner: false, edit: true });
		expect(abilities('read')).toEqual({ owner: false, edit: false });
	});
});

describe('upsertShare', () => {
	it('replaces a share with the same id, else appends, without mutating', () => {
		const list = [share('a'), share('b')];
		const changed = upsertShare(list, share('a', 'write'));
		expect(changed.map((s) => s.permission)).toEqual(['write', 'read']);
		expect(list[0].permission).toBe('read');
		expect(upsertShare(list, share('c')).map((s) => s.id)).toEqual(['a', 'b', 'c']);
	});
});

describe('replaceById / removeById', () => {
	it('swap or drop one entry', () => {
		const list = [comment('a'), comment('b')];
		expect(replaceById(list, comment('b', 'Edited')).map((c) => c.body)).toEqual(['Hi', 'Edited']);
		expect(removeById(list, 'a').map((c) => c.id)).toEqual(['b']);
		expect(list).toHaveLength(2);
	});
});

describe('mergeComments', () => {
	it('appends a newer page, skipping ones already shown', () => {
		expect(mergeComments([comment('a'), comment('b')], [comment('b'), comment('c')]).map((c) => c.id)).toEqual([
			'a',
			'b',
			'c'
		]);
	});
});

describe('commentProblem', () => {
	it('refuses empty and too long bodies', () => {
		expect(commentProblem('   ')).toBe('Write a comment first.');
		expect(commentProblem(' ok ')).toBeNull();
		expect(commentProblem('x'.repeat(COMMENT_MAX))).toBeNull();
		expect(commentProblem('x'.repeat(COMMENT_MAX + 1))).toBe(`Comments have at most ${COMMENT_MAX} characters.`);
	});

	it('counts characters, not UTF-16 units', () => {
		expect(commentProblem('😀'.repeat(COMMENT_MAX))).toBeNull();
	});
});

describe('commentWhen', () => {
	const now = new Date('2026-11-23T12:00:00Z');

	it('is relative within a day', () => {
		expect(commentWhen('2026-11-23T11:59:40Z', now)).toBe('Just now');
		expect(commentWhen('2026-11-23T11:55:00Z', now)).toBe('5 min ago');
		expect(commentWhen('2026-11-23T09:00:00Z', now)).toBe('3 h ago');
	});

	it('is a date beyond a day', () => {
		expect(commentWhen('2026-11-20T09:00:00Z', now)).not.toMatch(/ago|now/);
	});
});

describe('moveRoot', () => {
	const node = { id: 'here', name: 'Here' };

	it('is none for owners, else the shared folder', () => {
		expect(moveRoot('owner', [{ id: 'top', name: 'Top' }], node)).toBeNull();
		expect(moveRoot('write', [{ id: 'top', name: 'Top' }], node)).toEqual({ id: 'top', name: 'Top' });
		expect(moveRoot('write', [], node)).toEqual(node);
	});
});

describe('splitShared', () => {
	it('separates folders and files from albums', () => {
		const files = item({ share_id: '1', node: null });
		const photos = item({ share_id: '2', album: album('x') });
		const folder = item({
			share_id: '3',
			node: {
				id: 'n',
				name: 'Trip',
				kind: 'folder',
				is_root: false,
				size: 0,
				created_at: '',
				modified_at: ''
			}
		});
		const { nodes, albums } = splitShared([files, photos, folder]);
		expect(nodes.map((i) => i.share_id)).toEqual(['3']);
		expect(albums.map((i) => i.share_id)).toEqual(['2']);
	});
});

describe('writableAlbums', () => {
	it('lists own albums, then albums shared for writing, once each, by name', () => {
		const own = [album('b', 'Beach'), album('a', 'Alps')];
		const shared = [
			item({ permission: 'write', album: album('c', 'Cats') }),
			item({ permission: 'read', album: album('d', 'Dogs') }),
			item({ permission: 'write', album: album('a', 'Alps') })
		];
		expect(writableAlbums(own, shared).map((a) => a.id)).toEqual(['a', 'b', 'c']);
	});

	it('leaves out dynamic albums: their photos come from rules', () => {
		const own = [album('a', 'Alps'), album('d', 'Dogs', true)];
		const shared = [item({ permission: 'write', album: album('e', 'Eve', true) })];
		expect(writableAlbums(own, shared).map((a) => a.id)).toEqual(['a']);
	});
});

describe('sharedWithSummary', () => {
	const person = (name: string) => ({ username: name.toLowerCase(), display_name: name });

	it('names one, two or three people', () => {
		expect(sharedWithSummary([person('Bob')])).toBe('With Bob');
		expect(sharedWithSummary([person('Bob'), person('Carol')])).toBe('With Bob and Carol');
		expect(sharedWithSummary([person('Bob'), person('Carol'), person('Dan')])).toBe('With Bob, Carol and Dan');
	});

	it('counts the others past two names', () => {
		const people = ['Bob', 'Carol', 'Dan', 'Eve'].map(person);
		expect(sharedWithSummary(people)).toBe('With Bob, Carol and 2 others');
	});

	it('says nobody for an empty list', () => {
		expect(sharedWithSummary([])).toBe('With nobody');
	});
});
