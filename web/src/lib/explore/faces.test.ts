import { describe, expect, it } from 'vitest';
import { cropSource, cropStyle, personLabel } from '#lib/explore/faces.ts';

describe('face crops', () => {
	it('scales and moves the photo so the face fills the avatar', () => {
		const style = cropStyle({ x: 0.4, y: 0.2, width: 0.2, height: 0.25 }, 0);
		expect(style.height).toBeCloseTo(400);
		expect(style.top).toBeCloseTo(-80);
		expect(style.shift).toBeCloseTo(-50);
	});

	it('keeps room around the face', () => {
		const style = cropStyle({ x: 0, y: 0.5, width: 0.1, height: 0.2 }, 0.25);
		// 0.3 of the photo's height fills the avatar, starting at 0.45.
		expect(style.height).toBeCloseTo(100 / 0.3);
		expect(style.top).toBeCloseTo(-150);
		expect(style.shift).toBeCloseTo(-5);
	});

	it('crops small faces from the preview, none without a thumbnail', () => {
		const face = { face_id: 'f', photo_id: 'p', thumbnail_version: 2, box: { x: 0, y: 0, width: 0.1, height: 0.2 } };
		expect(cropSource(face)).toBe('/api/files/p/thumbnail?size=thumb&v=2');
		expect(cropSource({ ...face, box: { ...face.box, height: 0.05 } })).toBe('/api/files/p/thumbnail?size=preview&v=2');
		expect(cropSource({ ...face, thumbnail_version: null })).toBeNull();
	});

	it('names unnamed people', () => {
		expect(personLabel(null)).toBe('Unnamed person');
		expect(personLabel(' Anna ')).toBe('Anna');
	});
});
