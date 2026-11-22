import { describe, expect, it } from 'vitest';
import { bucketFor, previewPx } from '#lib/photos/previewSize.ts';

/** Shared with the phone app (`PreviewSizeTest.kt`): keep both tables identical. */
const VECTORS: { photo: [number, number] | null; viewport: [number, number]; dpr: number; px: number }[] = [
	// A 12 MP landscape photo on a 1080p screen, a 1366 × 768 one at 2×, then a 4K one.
	{ photo: [4000, 3000], viewport: [1920, 1080], dpr: 1, px: 1440 },
	{ photo: [4000, 3000], viewport: [1366, 768], dpr: 2, px: 2160 },
	{ photo: [4000, 3000], viewport: [3840, 2160], dpr: 1, px: 3200 },
	// A portrait photo on a phone (393 × 852 points at 3×).
	{ photo: [3024, 4032], viewport: [393, 852], dpr: 3, px: 2160 },
	// A landscape photo on the same phone: limited by the width.
	{ photo: [4032, 3024], viewport: [393, 852], dpr: 3, px: 1440 },
	// Never above the photo itself.
	{ photo: [1200, 800], viewport: [3840, 2160], dpr: 2, px: 1440 },
	{ photo: [2000, 1500], viewport: [3840, 2160], dpr: 2, px: 2160 },
	// Size unknown: the screen's long side.
	{ photo: null, viewport: [1280, 800], dpr: 2, px: 3200 },
	{ photo: null, viewport: [800, 600], dpr: 1, px: 1440 },
	// Nonsense ratios count as 1.
	{ photo: [4000, 3000], viewport: [1920, 1080], dpr: 0, px: 1440 }
];

describe('previewPx', () => {
	it.each(VECTORS)('$photo in $viewport at $dpr× asks for $px', ({ photo, viewport, dpr, px }) => {
		const size = photo && { width: photo[0], height: photo[1] };
		expect(previewPx(size, { width: viewport[0], height: viewport[1] }, dpr)).toBe(px);
	});

	it('rounds up to a bucket and caps at the largest', () => {
		expect(bucketFor(1)).toBe(1440);
		expect(bucketFor(1440)).toBe(1440);
		expect(bucketFor(1441)).toBe(2160);
		expect(bucketFor(10_000)).toBe(3200);
	});
});
