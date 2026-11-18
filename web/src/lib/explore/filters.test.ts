import { describe, expect, it } from 'vitest';
import {
	chips,
	filterFromParams,
	isEmptyFilter,
	normalizeFilter,
	photosHref,
	placeFilter,
	sameFilter,
	tagLabel,
	withFilter
} from '#lib/explore/filters.ts';

describe('filters', () => {
	it('round-trips through the address and keeps other keys', () => {
		const filter = {
			person: 'p1',
			tag: 'beach',
			country: 'FR',
			region: 'Île-de-France',
			city: 'Paris',
			camera_make: 'Canon',
			camera_model: 'EOS R6',
			kind: 'photo' as const
		};
		const url = withFilter('https://drive.local/search?q=dog', filter);
		expect(url.searchParams.get('q')).toBe('dog');
		expect(filterFromParams(url.searchParams)).toEqual(filter);
	});

	it('never produces a city or region without a country, nor a model without a make', () => {
		const params = new URLSearchParams('city=Paris&region=IDF&camera_model=R6&kind=video');
		expect(filterFromParams(params)).toEqual({});
		expect(normalizeFilter({ country: 'FR', region: '' })).toEqual({ country: 'FR', region: '' });
		expect(isEmptyFilter({ city: 'Paris' })).toBe(true);
	});

	it('removing one chip keeps the others', () => {
		const filter = { tag: 'dog', country: 'FR', city: 'Paris', camera_make: 'Canon' };
		const all = chips(filter);
		expect(all.map((chip) => chip.label)).toEqual(['Dog', 'Paris', 'Canon']);
		expect(all[1].without).toEqual({ tag: 'dog', camera_make: 'Canon' });
		expect(sameFilter(all[0].without, { country: 'FR', city: 'Paris', camera_make: 'Canon' })).toBe(true);
	});

	it('names people, tags and countries for chips', () => {
		expect(chips({ person: 'p1' }, () => 'Anna')[0].label).toBe('Anna');
		expect(chips({ person: 'p1' })[0].label).toBe('Unnamed');
		expect(tagLabel('birthday-cake')).toBe('Birthday cake');
		expect(chips({ country: 'FR' }, undefined)[0].label).toMatch(/France|FR/);
	});

	it('links to the filtered timeline', () => {
		expect(photosHref({ tag: 'dog' })).toBe('/photos?tag=dog');
		expect(photosHref({})).toBe('/photos');
	});

	it('builds a place filter the same way everywhere, keeping an empty region', () => {
		expect(placeFilter({ country_code: 'FR', region: null, city: null })).toEqual({ country: 'FR' });
		const noRegion = placeFilter({ country_code: 'FJ', region: null, city: 'Labasa' });
		expect(noRegion).toEqual({ country: 'FJ', region: '', city: 'Labasa' });
		const url = withFilter('https://drive.local/photos', noRegion);
		expect(url.searchParams.get('region')).toBe('');
		expect(filterFromParams(url.searchParams)).toEqual(noRegion);
	});
});
