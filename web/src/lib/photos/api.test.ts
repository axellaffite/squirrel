import { describe, expect, it } from 'vitest';
import { thumbnailUrl } from '#lib/photos/api.ts';

describe('thumbnailUrl', () => {
	it('carries the size and the cache-busting version', () => {
		expect(thumbnailUrl('a/b', 'thumb', 3)).toBe('/api/files/a%2Fb/thumbnail?size=thumb&v=3');
		expect(thumbnailUrl('id', 'preview', 1)).toBe('/api/files/id/thumbnail?size=preview&v=1');
	});
});
