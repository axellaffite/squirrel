import { describe, expect, it } from 'vitest';
import { contentUrl, previewUrl, thumbUrl, type LinkInfo, type PublicNode } from './api.ts';
import { fileOfNode, linkByline, linkName, zippable } from './view.ts';

const node = (over: Partial<PublicNode>): PublicNode => ({
	id: 'n1',
	parent_id: null,
	name: 'a.jpg',
	kind: 'file',
	size: 10,
	mime: 'image/jpeg',
	is_root: false,
	created_at: '2026-11-27T10:00:00Z',
	modified_at: '2026-11-27T10:00:00Z',
	thumbnail: null,
	...over
});

const info = (over: Partial<LinkInfo>): LinkInfo => ({ locked: false, ...over });

describe('public link URLs', () => {
	it('stay under the link and escape their parts', () => {
		expect(thumbUrl('tok', 'id1', 3)).toBe('/api/public/tok/files/id1/thumbnail?size=thumb&v=3');
		expect(previewUrl('tok', 'id1', 2879.6)).toBe('/api/public/tok/files/id1/thumbnail?size=preview&px=2880');
		expect(previewUrl('tok', 'id1', 0)).toBe('/api/public/tok/files/id1/thumbnail?size=preview&px=1');
		expect(contentUrl('tok', 'id1')).toBe('/api/public/tok/files/id1/content');
		expect(contentUrl('tok', 'id1', true)).toBe('/api/public/tok/files/id1/content?download=true');
		expect(contentUrl('a/b', 'c?d')).toBe('/api/public/a%2Fb/files/c%3Fd/content');
	});
});

describe('view helpers', () => {
	it('opens a node with or without a thumbnail', () => {
		expect(fileOfNode(node({}))).toEqual({ id: 'n1', name: 'a.jpg', mime: 'image/jpeg', hasThumbnail: false });
		expect(fileOfNode(node({ thumbnail: { version: 1, blurhash: null } })).hasThumbnail).toBe(true);
	});

	it('names and describes the link', () => {
		expect(linkName(info({ node: node({ name: 'Trip', kind: 'folder' }) }))).toBe('Trip');
		expect(linkName(info({ album: { id: 'a', name: 'Party', item_count: 2, cover_id: null } }))).toBe('Party');
		expect(linkByline(info({ shared_by: 'Alice' }))).toBe('Shared by Alice');
		expect(linkByline(info({ shared_by: 'Alice', expires_at: '2026-12-04T12:00:00Z' }))).toMatch(/^Shared by Alice · Until /);
	});

	it('zips files only', () => {
		expect(zippable([node({ id: 'f' }), node({ id: 'd', kind: 'folder' })])).toEqual(['f']);
	});
});
