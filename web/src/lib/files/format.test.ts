import { describe, expect, it } from 'vitest';
import { contentUrl, formatBytes, kindLabel, previewKind, storageSummary } from '#lib/files/format.ts';

describe('formatBytes', () => {
	it.each([
		[0, '0 B'],
		[999, '999 B'],
		[1500, '1.5 KB'],
		[9_437_184, '9.4 MB'],
		[123_456_789_012, '123 GB'],
		[-1, '—']
	])('%d → %s', (bytes, expected) => expect(formatBytes(bytes)).toBe(expected));
});

describe('previewKind', () => {
	it.each([
		['application/pdf', 'pdf'],
		['image/png', 'image'],
		['image/svg+xml', null],
		['video/mp4', 'video'],
		['video/quicktime', null],
		['audio/mpeg', 'audio'],
		['text/plain', 'text'],
		['text/csv', 'text'],
		['application/json', 'text'],
		['text/html', null],
		['application/zip', null],
		[null, null]
	])('%s → %s', (mime, expected) => expect(previewKind(mime)).toBe(expected));
});

describe('contentUrl', () => {
	it('builds inline and download URLs', () => {
		expect(contentUrl('abc')).toBe('/api/files/abc/content');
		expect(contentUrl('abc', true)).toBe('/api/files/abc/content?download=true');
	});
});

describe('storageSummary', () => {
	it('mentions the quota only when there is one', () => {
		expect(storageSummary({ used_bytes: 1500, quota_bytes: null })).toBe('1.5 KB used');
		expect(storageSummary({ used_bytes: 1500, quota_bytes: 1_000_000 })).toBe('1.5 KB of 1.0 MB used');
	});
});

describe('kindLabel', () => {
	it.each([
		[{ kind: 'folder', name: 'Biarritz, August' }, 'Folder'],
		[{ kind: 'file', name: 'lease-2026.pdf' }, 'PDF'],
		[{ kind: 'file', name: 'IMG_4021.heic' }, 'HEIC'],
		[{ kind: 'file', name: 'archive.tar.gz' }, 'GZ'],
		[{ kind: 'file', name: 'README' }, 'File'],
		[{ kind: 'file', name: '.env' }, 'File'],
		[{ kind: 'file', name: 'notes.verylongextension' }, 'File']
	] as const)('%o → %s', (node, expected) => expect(kindLabel(node)).toBe(expected));
});
