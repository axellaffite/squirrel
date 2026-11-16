import { describe, expect, it } from 'vitest';
import { photoInAddress, withPhoto } from '#lib/photos/address.ts';

describe('photo address', () => {
	it('sets, replaces and removes the photo, keeping other parameters', () => {
		const shown = withPhoto('http://x/photos/albums/a?tab=1', 'p1');
		expect(shown.href).toBe('http://x/photos/albums/a?tab=1&photo=p1');
		expect(withPhoto(shown.href, 'p2').searchParams.get('photo')).toBe('p2');
		expect(withPhoto(shown.href, null).href).toBe('http://x/photos/albums/a?tab=1');
	});

	it('reads the photo, ignoring an empty value', () => {
		expect(photoInAddress(new URL('http://x/photos?photo=p1'))).toBe('p1');
		expect(photoInAddress(new URL('http://x/photos?photo='))).toBeNull();
		expect(photoInAddress(new URL('http://x/photos'))).toBeNull();
	});
});
