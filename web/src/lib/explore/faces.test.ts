import { describe, expect, it } from 'vitest';
import { cropSource, cropStyle, cropWindow, faceName, personLabel, readingOrder } from '#lib/explore/faces.ts';

describe('face crops', () => {
	it('scales and moves the photo so the face fills the avatar', () => {
		const style = cropStyle({ x: 0.4, y: 0.2, width: 0.2, height: 0.25 }, 1, 0);
		expect(style.height).toBeCloseTo(400);
		expect(style.top).toBeCloseTo(-80);
		expect(style.shift).toBeCloseTo(-50);
	});

	it('keeps room around the face', () => {
		const style = cropStyle({ x: 0.4, y: 0.5, width: 0.1, height: 0.2 }, 1, 0.25);
		// 0.3 of the photo's height fills the avatar, starting at 0.45.
		expect(style.height).toBeCloseTo(100 / 0.3);
		expect(style.top).toBeCloseTo(-150);
		expect(style.shift).toBeCloseTo(-45);
	});

	it('never shows past the photo’s edges', () => {
		// A face in the top-left corner of a landscape photo (3:2).
		const corner = cropWindow({ x: 0, y: 0, width: 0.1, height: 0.2 }, 1.5, 0.25);
		expect(corner.top).toBe(0);
		expect(corner.centre).toBeCloseTo(corner.width / 2);
		// A face at the bottom-right edge.
		const edge = cropWindow({ x: 0.95, y: 0.9, width: 0.05, height: 0.1 }, 1.5, 0.25);
		expect(edge.top + edge.height).toBeCloseTo(1);
		expect(edge.centre + edge.width / 2).toBeCloseTo(1);
		// A face taller than the photo allows: the whole height.
		expect(cropWindow({ x: 0.2, y: 0, width: 0.5, height: 0.9 }).height).toBe(1);
	});

	it('crops a face on a turned photo in displayed coordinates', () => {
		// A portrait shown upright from a sideways file (EXIF orientation 6): the box and the
		// thumbnail are both in the displayed (portrait, 2:3) frame, so nothing is turned here.
		const window = cropWindow({ x: 0.35, y: 0.2, width: 0.3, height: 0.2 }, 2 / 3, 0.25);
		expect(window.height).toBeCloseTo(0.3);
		expect(window.width).toBeCloseTo(0.45);
		expect(window.centre).toBeCloseTo(0.5);
	});

	it('crops from the preview when the thumb is too coarse for the avatar', () => {
		const face = { face_id: 'f', photo_id: 'p', thumbnail_version: 2, box: { x: 0, y: 0, width: 0.2, height: 0.4 } };
		// 0.6 × 360 = 216 thumb pixels: enough for 40 and 64 px avatars at 2×, not for 112 px.
		expect(cropSource(face, 40)).toBe('/api/files/p/thumbnail?size=thumb&v=2');
		expect(cropSource(face, 112)).toBe('/api/files/p/thumbnail?size=preview&v=2');
		expect(cropSource({ ...face, box: { ...face.box, height: 0.05 } }, 40)).toBe('/api/files/p/thumbnail?size=preview&v=2');
		expect(cropSource({ ...face, thumbnail_version: null }, 40)).toBeNull();
	});

	it('names unnamed people', () => {
		expect(personLabel(null)).toBe('Unnamed');
		expect(personLabel(' Anna ')).toBe('Anna');
	});

	it('orders faces as they are read: rows, then left to right', () => {
		const face = (id: string, x: number, y: number) => ({ id, box: { x, y, width: 0.1, height: 0.2 } });
		const order = readingOrder([face('c', 0.1, 0.6), face('b', 0.7, 0.12), face('a', 0.2, 0.1)]);
		expect(order.map((f) => f.id)).toEqual(['a', 'b', 'c']);
	});

	it('names face boxes', () => {
		expect(faceName('Anna')).toBe('Face of Anna');
		expect(faceName(null)).toBe('Unnamed face');
	});
});
