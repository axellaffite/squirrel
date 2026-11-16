import { describe, expect, it } from 'vitest';
import { formatCoordinates, openStreetMapUrl, project } from '#lib/photos/geo.ts';

describe('project', () => {
	it('maps latitude and longitude onto the 360 × 180 map', () => {
		const paris = project(48.8584, 2.2945);
		expect(paris.x).toBeCloseTo(182.2945, 6);
		expect(paris.y).toBeCloseTo(41.1416, 6);
		expect(project(0, 0)).toEqual({ x: 180, y: 90 });
	});

	it('wraps across the antimeridian and clamps at the poles', () => {
		expect(project(0, 180).x).toBe(0);
		expect(project(0, -180).x).toBe(0);
		expect(project(0, 190).x).toBe(10);
		expect(project(0, -190).x).toBe(350);
		expect(project(95, 0).y).toBe(0);
		expect(project(-90, 0).y).toBe(180);
	});
});

describe('coordinates', () => {
	it('prints six decimals', () => {
		expect(formatCoordinates(48.8584, 2.2945)).toBe('48.858400, 2.294500');
		expect(formatCoordinates(-33.8568, 151.2153)).toBe('-33.856800, 151.215300');
	});

	it('links to OpenStreetMap', () => {
		expect(openStreetMapUrl(48.8584, 2.2945)).toBe(
			'https://www.openstreetmap.org/?mlat=48.858400&mlon=2.294500#map=15/48.858400/2.294500'
		);
	});
});
