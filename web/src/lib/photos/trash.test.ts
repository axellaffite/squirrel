import { describe, expect, it } from 'vitest';
import { neighbourAfterRemoval, trashPhotos } from '#lib/photos/trash.ts';

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

describe('neighbourAfterRemoval', () => {
	const order = [{ id: 'a' }, { id: 'b' }, { id: 'c' }];

	it('shows the next photo, else the previous one', () => {
		expect(neighbourAfterRemoval(order, 'b')).toEqual({ id: 'c' });
		expect(neighbourAfterRemoval(order, 'c')).toEqual({ id: 'b' });
		expect(neighbourAfterRemoval([{ id: 'a' }], 'a')).toBeNull();
	});

	it('closes the viewer for a photo that is not in the loaded order', () => {
		expect(neighbourAfterRemoval(order, 'z')).toBeNull();
	});
});
