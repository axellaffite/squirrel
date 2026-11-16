import { describe, expect, it } from 'vitest';
import { addedMessage, batches, photoCount, sortByName } from '#lib/photos/albums.ts';

describe('albums', () => {
	it('sorts by name, ignoring case and accents, numbers in order', () => {
		const albums = [{ name: 'été 2024' }, { name: 'Album 10' }, { name: 'album 9' }, { name: 'Ete 2023' }];
		expect(sortByName(albums, 'fr').map((a) => a.name)).toEqual(['album 9', 'Album 10', 'Ete 2023', 'été 2024']);
		expect(albums[0].name).toBe('été 2024');
	});

	it('counts photos', () => {
		expect(photoCount(1)).toBe('1 photo');
		expect(photoCount(0)).toBe('0 photos');
	});

	it('says what was added', () => {
		expect(addedMessage(3, 3, 'Holidays')).toBe('Added 3 photos to Holidays.');
		expect(addedMessage(0, 1, 'Holidays')).toBe('This photo is already in Holidays.');
		expect(addedMessage(0, 2, 'Holidays')).toBe('These photos are already in Holidays.');
	});

	it('splits requests', () => {
		expect(batches([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
		expect(batches([], 2)).toEqual([]);
	});
});
