import { describe, expect, it } from 'vitest';
import { browserCanShow, cameraName, dimensions, exposureLine, placeLine } from '#lib/photos/info.ts';

describe('info panel wording', () => {
	it('names the place', () => {
		expect(placeLine({ name: 'Paris', admin1: 'Île-de-France', country: 'France', country_code: 'FR' })).toBe(
			'Paris, Île-de-France, France'
		);
		expect(placeLine({ name: 'Monaco', admin1: 'Monaco', country: 'Monaco' })).toBe('Monaco');
		expect(placeLine({ name: 'Somewhere' })).toBe('Somewhere');
	});

	it('names the camera without repeating the make', () => {
		expect(cameraName({ make: 'Google', model: 'Pixel 8' })).toBe('Google Pixel 8');
		expect(cameraName({ make: 'Canon', model: 'Canon EOS R6' })).toBe('Canon EOS R6');
		expect(cameraName({ make: 'Apple' })).toBe('Apple');
		expect(cameraName({})).toBeNull();
	});

	it('writes one exposure line from what was recorded', () => {
		expect(exposureLine({ exposure_time: '1/250', f_number: 1.7, iso: 64, focal_length_mm: 6.9 })).toBe(
			'1/250 · ƒ/1.7 · ISO 64 · 6.9 mm'
		);
		expect(exposureLine({ f_number: 2.8000001 })).toBe('ƒ/2.8');
		expect(exposureLine({})).toBeNull();
	});

	it('writes dimensions and knows what the browser shows', () => {
		expect(dimensions(4032, 3024)).toBe('4032 × 3024');
		expect(dimensions(null, 3024)).toBeNull();
		expect(browserCanShow('image/jpeg')).toBe(true);
		expect(browserCanShow('image/heic')).toBe(false);
		expect(browserCanShow(null)).toBe(false);
	});
});
