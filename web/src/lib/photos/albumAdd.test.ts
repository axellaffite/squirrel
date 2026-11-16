import { describe, expect, it, vi } from 'vitest';
import { AlbumAdder, type AlbumWrites } from '#lib/photos/albumAdd.ts';

const ids = (count: number) => Array.from({ length: count }, (_, i) => `p${i}`);
const album = (id: string, name: string, count: number) =>
	({ id, name, item_count: count, updated_at: '' }) as never;

function writes(): AlbumWrites & { createAlbum: ReturnType<typeof vi.fn>; addToAlbum: ReturnType<typeof vi.fn> } {
	return {
		createAlbum: vi.fn(async (name: string, nodeIds: readonly string[]) => ({
			ok: true as const,
			data: album('new', name, nodeIds.length)
		})),
		addToAlbum: vi.fn(async (_id: string, nodeIds: readonly string[]) => ({
			ok: true as const,
			data: { changed: nodeIds.length }
		}))
	};
}

describe('AlbumAdder', () => {
	it('creates the album with the first batch and adds the rest, counting what changed', async () => {
		const w = writes();
		const result = await new AlbumAdder(ids(1200), w).intoNew('Holidays');
		expect(w.createAlbum).toHaveBeenCalledOnce();
		expect(w.createAlbum.mock.calls[0][1]).toHaveLength(500);
		expect(w.addToAlbum.mock.calls.map((call) => call[1].length)).toEqual([500, 200]);
		expect(result).toEqual({ ok: true, message: 'Added 1200 photos to Holidays.' });
	});

	it('never creates the album twice when a later batch failed and the user retries', async () => {
		const w = writes();
		w.addToAlbum.mockResolvedValueOnce({ ok: false, error: 'busy', status: 503 });
		const adder = new AlbumAdder(ids(1200), w);
		expect(await adder.intoNew('Holidays')).toEqual({ ok: false, error: 'busy' });
		const retry = await adder.intoNew('Holidays');
		expect(w.createAlbum).toHaveBeenCalledOnce();
		// Batch 2 failed then succeeded on retry; batch 3 sent once.
		expect(w.addToAlbum.mock.calls.map((call) => call[1][0])).toEqual(['p500', 'p500', 'p1000']);
		expect(retry).toEqual({ ok: true, message: 'Added 1200 photos to Holidays.' });
	});

	it('sums the server counts for an existing album', async () => {
		const w = writes();
		w.addToAlbum.mockResolvedValueOnce({ ok: true, data: { changed: 1 } });
		const result = await new AlbumAdder(ids(3), w).into({ id: 'a', name: 'Kids' });
		expect(result).toEqual({ ok: true, message: 'Added 1 photo to Kids.' });
	});

	it('reports a failed creation without remembering an album', async () => {
		const w = writes();
		w.createAlbum.mockResolvedValueOnce({ ok: false, error: 'Bad name', status: 422 });
		const adder = new AlbumAdder(ids(2), w);
		expect(await adder.intoNew('x')).toEqual({ ok: false, error: 'Bad name' });
		expect(adder.created).toBeNull();
	});
});
