import { describe, expect, it } from 'vitest';
import { contentUrl, previewUrl, thumbUrl, type LinkInfo, type PublicNode } from './api.ts';
import { fileOfNode, linkDetails, linkExpiry, linkIcon, linkName, zippable } from './view.ts';

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
	});

	it('details the link: who shared it, the file size or the photo count', () => {
		expect(linkDetails(info({ shared_by: 'Alice', node: node({ name: 'Trip', kind: 'folder' }) }))).toBe('Shared by Alice');
		expect(linkDetails(info({ shared_by: 'Alice', node: node({ size: 2048 }) }))).toBe('Shared by Alice · 2.0 KB');
		const album = (count: number) => ({ id: 'a', name: 'Party', item_count: count, cover_id: null });
		expect(linkDetails(info({ shared_by: 'Alice', album: album(1) }))).toBe('Shared by Alice · 1 photo');
		expect(linkDetails(info({ shared_by: 'Alice', album: album(12) }))).toBe('Shared by Alice · 12 photos');
		expect(linkDetails(info({}))).toBe('');
	});

	it('says when the link expires, if it does', () => {
		expect(linkExpiry(info({}))).toBeNull();
		expect(linkExpiry(info({ expires_at: '2026-12-04T12:00:00Z' }))).toMatch(/^This link expires on /);
	});

	it('picks an icon for the link', () => {
		expect(linkIcon(info({ node: node({ kind: 'folder' }) }))).toBe('folder');
		expect(linkIcon(info({ node: node({}) }))).toBe('image');
		expect(linkIcon(info({ album: { id: 'a', name: 'Party', item_count: 0, cover_id: null } }))).toBe('images');
	});

	it('zips files only', () => {
		expect(zippable([node({ id: 'f' }), node({ id: 'd', kind: 'folder' })])).toEqual(['f']);
	});
});
