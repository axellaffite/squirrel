import { describe, expect, it, vi } from 'vitest';
import { AlbumData, type AlbumSource } from '#lib/photos/album.svelte.ts';

const photo = (id: string) => ({ id }) as never;
const album = { id: 'a', name: 'Kids', item_count: 3, updated_at: '' } as never;

function source(): AlbumSource & Record<keyof AlbumSource, ReturnType<typeof vi.fn>> {
	return {
		getAlbum: vi.fn(async () => ({ ok: true as const, data: album })),
		listAlbumPage: vi.fn(async (_id: string, cursor: string | null) => ({
			ok: true as const,
			data: cursor === null ? { items: [photo('1'), photo('2')], next_cursor: 'c2' } : { items: [photo('3')], next_cursor: null }
		})),
		removeFromAlbum: vi.fn(async (_id: string, ids: readonly string[]) => ({ ok: true as const, data: { changed: ids.length } }))
	};
}

const ids = (data: AlbumData) => data.items.map((item) => (item as { id: string }).id);

describe('AlbumData', () => {
	it('pages through the photos', async () => {
		const data = new AlbumData('a', source());
		await data.reload();
		await data.nextPage();
		expect(ids(data)).toEqual(['1', '2', '3']);
		expect(data.finished).toBe(true);
	});

	it('drops a page that a reload overtook and starts again from page one', async () => {
		const src = source();
		let release: (() => void) | undefined;
		src.listAlbumPage.mockImplementationOnce(
			() => new Promise((resolve) => (release = () => resolve({ ok: true, data: { items: [photo('old')], next_cursor: 'x' } })))
		);
		const data = new AlbumData('a', src);
		const first = data.nextPage();
		await data.reload();
		release?.();
		await first;
		expect(ids(data)).toEqual(['1', '2']);
	});

	it('keeps the album error when a page fails too', async () => {
		const src = source();
		src.getAlbum.mockResolvedValueOnce({ ok: false, error: 'gone', status: 404 });
		src.listAlbumPage.mockResolvedValueOnce({ ok: false, error: 'boom', status: 500 });
		const data = new AlbumData('a', src);
		await data.reload();
		expect(data.albumError).toBe('This album does not exist or was deleted.');
		expect(data.pageError).toBe('boom');
	});

	it('removes in batches and reports a partial failure', async () => {
		const src = source();
		src.removeFromAlbum
			.mockResolvedValueOnce({ ok: true, data: { changed: 500 } })
			.mockResolvedValueOnce({ ok: false, error: 'busy', status: 503 });
		const data = new AlbumData('a', src);
		const many = Array.from({ length: 1200 }, (_, i) => `p${i}`);
		const result = await data.remove(many);
		expect(src.removeFromAlbum).toHaveBeenCalledTimes(2);
		expect(result).toEqual({ removed: 500, message: 'Removed 500 photos from the album.', error: 'busy' });
		expect(ids(data)).toEqual(['1', '2']);
	});
});
