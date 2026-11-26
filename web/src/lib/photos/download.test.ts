import { describe, expect, it, vi } from 'vitest';
import {
	archiveError,
	archiveUrl,
	downloadLabel,
	downloadPhotos,
	MAX_ARCHIVE,
	planDownload,
	type ArchiveCall
} from '#lib/photos/download.ts';

const ticket = { ticket: 't-1', name: 'Squirrel 2 photos 2026-11-26.zip', size: 1234, count: 2 };

describe('planDownload', () => {
	it('downloads one photo directly, as the original', () => {
		expect(planDownload(['a'])).toEqual({ kind: 'direct', url: '/api/files/a/content?download=true' });
	});

	it('zips several photos, keeping their order', () => {
		expect(planDownload(['b', 'a', 'c'])).toEqual({ kind: 'archive', body: { node_ids: ['b', 'a', 'c'] } });
	});

	it('does nothing without a selection', () => {
		expect(planDownload([])).toEqual({ kind: 'none' });
	});

	it('refuses more than one zip can hold, without asking the server', () => {
		const ids = Array.from({ length: MAX_ARCHIVE + 1 }, (_, index) => `id-${index}`);
		expect(planDownload(ids)).toEqual({ kind: 'too-many', message: `Select at most ${MAX_ARCHIVE} photos to download at once.` });
		expect(planDownload(ids.slice(0, MAX_ARCHIVE)).kind).toBe('archive');
	});
});

describe('archiveUrl', () => {
	it('points at the ticket, escaped', () => {
		expect(archiveUrl('abc')).toBe('/api/files/archive/abc');
		expect(archiveUrl('a/b')).toBe('/api/files/archive/a%2Fb');
	});
});

describe('downloadLabel', () => {
	it('names the photo when there is one', () => {
		expect(downloadLabel([{ id: 'a', name: 'IMG_1.jpg' }], 1)).toBe('Download IMG_1.jpg');
	});

	it('counts the photos when there are several', () => {
		expect(downloadLabel([], 12)).toBe('Download 12 photos');
	});

	it('falls back to a count when the one photo is not loaded', () => {
		expect(downloadLabel([], 1)).toBe('Download 1 photo');
	});
});

describe('archiveError', () => {
	it('explains a zip that would be too large', () => {
		const body = { error: { code: 'archive_too_large', message: 'too much to put in one zip: select fewer files' } };
		expect(archiveError(body, 413)).toBe('Too much to put in one zip: select fewer photos.');
	});

	it('explains photos that are gone', () => {
		expect(archiveError({ error: { code: 'not_found', message: 'not found' } }, 404)).toBe(
			'Some of these photos are no longer available. Reload the page and try again.'
		);
	});

	it('uses the friendly busy message', () => {
		expect(archiveError({ error: { code: 'busy', message: 'busy' } }, 503)).toBe('The server is busy. Try again in a moment.');
	});

	it('falls back to the server message, then a generic one', () => {
		expect(archiveError({ error: { code: 'bad_request', message: 'folders cannot be put in a zip' } }, 400)).toBe(
			'folders cannot be put in a zip'
		);
		expect(archiveError(null, 0)).toBe('Something went wrong. Please try again.');
	});
});

describe('downloadPhotos', () => {
	it('starts the original of one photo without asking for a zip', async () => {
		const createArchive = vi.fn<ArchiveCall>();
		const start = vi.fn();
		expect(await downloadPhotos(['a'], { createArchive, start })).toBeNull();
		expect(createArchive).not.toHaveBeenCalled();
		expect(start).toHaveBeenCalledWith('/api/files/a/content?download=true');
	});

	it('asks for a zip of several photos, then starts its download', async () => {
		const createArchive = vi.fn<ArchiveCall>(async () => ({ ok: true, data: ticket }));
		const start = vi.fn();
		expect(await downloadPhotos(['a', 'b'], { createArchive, start })).toBeNull();
		expect(createArchive).toHaveBeenCalledWith({ node_ids: ['a', 'b'] });
		expect(start).toHaveBeenCalledWith('/api/files/archive/t-1');
	});

	it('reports a refused zip and starts nothing', async () => {
		const createArchive = vi.fn<ArchiveCall>(async () => ({ ok: false, error: 'Too much.', status: 413 }));
		const start = vi.fn();
		expect(await downloadPhotos(['a', 'b'], { createArchive, start })).toBe('Too much.');
		expect(start).not.toHaveBeenCalled();
	});

	it('reports too many photos without a request', async () => {
		const createArchive = vi.fn<ArchiveCall>();
		const start = vi.fn();
		const ids = Array.from({ length: MAX_ARCHIVE + 1 }, (_, index) => `id-${index}`);
		expect(await downloadPhotos(ids, { createArchive, start })).toBe(`Select at most ${MAX_ARCHIVE} photos to download at once.`);
		expect(createArchive).not.toHaveBeenCalled();
		expect(start).not.toHaveBeenCalled();
	});

	it('does nothing for an empty selection', async () => {
		const createArchive = vi.fn<ArchiveCall>();
		const start = vi.fn();
		expect(await downloadPhotos([], { createArchive, start })).toBeNull();
		expect(createArchive).not.toHaveBeenCalled();
		expect(start).not.toHaveBeenCalled();
	});
});
