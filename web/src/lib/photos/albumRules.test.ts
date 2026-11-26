import { describe, expect, it } from 'vitest';
import {
	MAX_RULE_ITEMS,
	emptyRulesForm,
	formFromRules,
	hasCondition,
	ruleParts,
	rulesBody,
	rulesProblem,
	withItem,
	withoutItem,
	type RulesForm
} from '#lib/photos/albumRules.ts';

const form = (over: Partial<RulesForm> = {}): RulesForm => ({ ...emptyRulesForm(), ...over });

describe('emptyRulesForm', () => {
	it('constrains nothing and matches any', () => {
		expect(emptyRulesForm()).toEqual({
			people: [],
			peopleMatch: 'any',
			tags: [],
			tagsMatch: 'any',
			place: null,
			camera: null,
			kind: 'any',
			takenFrom: '',
			takenTo: ''
		});
		expect(hasCondition(emptyRulesForm())).toBe(false);
	});
});

describe('formFromRules', () => {
	it('reads every field the server returns', () => {
		const result = formFromRules({
			people: ['p1'],
			people_match: 'all',
			tags: ['beach', 'sunset'],
			tags_match: 'all',
			country: 'FR',
			region: 'Brittany',
			city: 'Brest',
			camera_make: 'Canon',
			camera_model: 'EOS R5',
			kind: 'photo',
			taken_from: '2024-01-01',
			taken_to: '2024-12-31'
		});
		expect(result).toEqual({
			people: ['p1'],
			peopleMatch: 'all',
			tags: ['beach', 'sunset'],
			tagsMatch: 'all',
			place: { country: 'FR', region: 'Brittany', city: 'Brest' },
			camera: { make: 'Canon', model: 'EOS R5' },
			kind: 'photo',
			takenFrom: '2024-01-01',
			takenTo: '2024-12-31'
		});
	});

	it('keeps an empty region (places without one) and fills the defaults', () => {
		const result = formFromRules({ country: 'FR', region: '', city: 'Paris' });
		expect(result.place).toEqual({ country: 'FR', region: '', city: 'Paris' });
		expect(result.peopleMatch).toBe('any');
		expect(result.kind).toBe('any');
	});

	it('drops a region or city without a country, and a model without a make', () => {
		const result = formFromRules({ region: 'Brittany', city: 'Brest', camera_model: 'EOS R5' });
		expect(result.place).toBeNull();
		expect(result.camera).toBeNull();
	});

	it('gives an empty form for no rules', () => {
		expect(formFromRules(null)).toEqual(emptyRulesForm());
		expect(formFromRules(undefined)).toEqual(emptyRulesForm());
	});
});

describe('rulesBody', () => {
	it('sends only the conditions that constrain', () => {
		expect(rulesBody(form({ tags: ['beach'] }))).toEqual({ people: [], people_match: 'any', tags: ['beach'], tags_match: 'any' });
	});

	it('sends a place, a camera, a kind and dates', () => {
		const body = rulesBody(
			form({
				place: { country: 'FR', region: '', city: 'Paris' },
				camera: { make: 'Canon', model: 'EOS R5' },
				kind: 'screenshot',
				takenFrom: '2024-01-01',
				takenTo: '2024-02-01'
			})
		);
		expect(body).toEqual({
			people: [],
			people_match: 'any',
			tags: [],
			tags_match: 'any',
			country: 'FR',
			region: '',
			city: 'Paris',
			camera_make: 'Canon',
			camera_model: 'EOS R5',
			kind: 'screenshot',
			taken_from: '2024-01-01',
			taken_to: '2024-02-01'
		});
	});

	it('leaves out a country-only place’s region and city, and a make-only camera’s model', () => {
		const body = rulesBody(form({ place: { country: 'FR' }, camera: { make: 'Canon' } }));
		expect(body).toMatchObject({ country: 'FR', camera_make: 'Canon' });
		expect(body).not.toHaveProperty('region');
		expect(body).not.toHaveProperty('city');
		expect(body).not.toHaveProperty('camera_model');
	});

	it('round-trips with formFromRules', () => {
		const original = form({
			people: ['a', 'b'],
			peopleMatch: 'all',
			tags: ['dog'],
			place: { country: 'JP', region: 'Tokyo', city: 'Shibuya' },
			takenFrom: '2023-05-01'
		});
		expect(formFromRules(rulesBody(original))).toEqual(original);
	});
});

describe('rulesProblem', () => {
	it('needs at least one condition', () => {
		expect(rulesProblem(emptyRulesForm())).toBe('Choose at least one condition.');
	});

	it('accepts any single condition', () => {
		expect(rulesProblem(form({ people: ['p'] }))).toBeNull();
		expect(rulesProblem(form({ tags: ['t'] }))).toBeNull();
		expect(rulesProblem(form({ place: { country: 'FR' } }))).toBeNull();
		expect(rulesProblem(form({ camera: { make: 'Canon' } }))).toBeNull();
		expect(rulesProblem(form({ kind: 'photo' }))).toBeNull();
		expect(rulesProblem(form({ takenFrom: '2024-01-01' }))).toBeNull();
		expect(rulesProblem(form({ takenTo: '2024-01-01' }))).toBeNull();
	});

	it('limits people and tags', () => {
		const many = Array.from({ length: MAX_RULE_ITEMS + 1 }, (_, i) => `x${i}`);
		expect(rulesProblem(form({ people: many }))).toBe(`Choose at most ${MAX_RULE_ITEMS} people.`);
		expect(rulesProblem(form({ tags: many }))).toBe(`Choose at most ${MAX_RULE_ITEMS} tags.`);
	});

	it('refuses dates that are not real days', () => {
		expect(rulesProblem(form({ takenFrom: '2024-02-30' }))).toBe('Enter the start date as a real day.');
		expect(rulesProblem(form({ takenTo: 'soon' }))).toBe('Enter the end date as a real day.');
	});

	it('refuses a range that ends before it starts', () => {
		expect(rulesProblem(form({ takenFrom: '2024-03-02', takenTo: '2024-03-01' }))).toBe('The end date is before the start date.');
		expect(rulesProblem(form({ takenFrom: '2024-03-01', takenTo: '2024-03-01' }))).toBeNull();
	});
});

describe('withItem / withoutItem', () => {
	it('adds once, at the end, without changing the list given', () => {
		const list = ['a'];
		expect(withItem(list, 'b')).toEqual(['a', 'b']);
		expect(withItem(list, 'a')).toBe(list);
		expect(list).toEqual(['a']);
	});

	it('stops at the limit', () => {
		const full = Array.from({ length: MAX_RULE_ITEMS }, (_, i) => `x${i}`);
		expect(withItem(full, 'more')).toBe(full);
	});

	it('removes', () => {
		expect(withoutItem(['a', 'b'], 'a')).toEqual(['b']);
	});
});

describe('ruleParts', () => {
	it('describes each condition in order, naming people', () => {
		const parts = ruleParts(
			form({
				people: ['p1', 'p2'],
				peopleMatch: 'all',
				tags: ['birthday-cake'],
				place: { country: 'FR', region: '', city: 'Paris' },
				camera: { make: 'Canon', model: 'EOS R5' },
				kind: 'screenshot',
				takenFrom: '2024-01-01',
				takenTo: '2024-12-31'
			}),
			(id) => (id === 'p1' ? 'Alice' : null),
			'en-GB'
		);
		expect(parts).toEqual([
			'Alice and Unnamed',
			'Birthday cake',
			'Paris',
			'Canon EOS R5',
			'Screenshots',
			'1 Jan 2024 – 31 Dec 2024'
		]);
	});

	it('says "or" for any, and names a country, an open-ended range', () => {
		const parts = ruleParts(
			form({ tags: ['dog', 'cat', 'bird'], place: { country: 'FR' }, takenFrom: '2024-01-01' }),
			() => null,
			'en-GB'
		);
		expect(parts).toEqual(['Dog, Cat or Bird', 'France', 'Since 1 Jan 2024']);
		expect(ruleParts(form({ takenTo: '2024-01-01' }), () => null, 'en-GB')).toEqual(['Until 1 Jan 2024']);
	});
});
