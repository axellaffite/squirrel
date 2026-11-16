import { describe, expect, it } from 'vitest';
import { trashPhotos } from '#lib/photos/trash.ts';

describe('trashPhotos', () => {
	it('reports what moved and what failed', async () => {
		const result = await trashPhotos(['a', 'b', 'c'], async (id) =>
			id === 'b' ? { ok: false, error: 'Not found.', status: 404 } : { ok: true, data: null }
		);
		expect(result).toEqual({ moved: ['a', 'c'], error: '1 photo was not moved to the trash. Not found.' });
	});

	it('is quiet when everything moved', async () => {
		const result = await trashPhotos(['a'], async () => ({ ok: true, data: null }));
		expect(result).toEqual({ moved: ['a'], error: null });
	});
});
