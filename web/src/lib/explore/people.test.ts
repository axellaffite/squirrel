import { describe, expect, it } from 'vitest';
import { cleanName, emptyReason, mergeReason, mergeTarget } from '#lib/explore/people.ts';

const person = (id: string, name: string | null, face_count: number) => ({ id, name, face_count });

describe('people', () => {
	it('keeps a named person in a merge, else the one with most faces', () => {
		expect(mergeTarget([person('a', null, 9), person('b', 'Anna', 2), person('c', 'Ben', 5)])?.id).toBe('c');
		expect(mergeTarget([person('a', null, 3), person('b', null, 7)])?.id).toBe('b');
		expect(mergeTarget([person('a', null, 3), person('b', ' ', 3)])?.id).toBe('a');
		expect(mergeTarget([])).toBeNull();
	});

	it('says why a person is kept by default', () => {
		const anna = person('b', 'Anna', 2);
		expect(mergeReason(anna, [person('a', null, 9), anna])).toBe('Chosen because they are the only one with a name.');
		expect(mergeReason(anna, [person('c', 'Ben', 1), anna])).toBe('Chosen because they are named and have the most photos.');
		expect(mergeReason(person('a', null, 9), [person('a', null, 9), person('d', null, 1)])).toBe('Chosen because they have the most photos.');
	});

	it('cleans names', () => {
		expect(cleanName('  Anna ')).toBe('Anna');
		expect(cleanName('   ')).toBeNull();
		expect(cleanName('é'.repeat(101))).toBeNull();
		expect(cleanName('An\u0007na')).toBeNull();
		expect(cleanName('Ann\ta')).toBeNull();
	});

	it('explains an empty people grid', () => {
		expect(emptyReason({ facesEnabled: false, facesInstalled: true, analysing: 3 })?.title).toBe('Face recognition is off');
		expect(emptyReason({ facesEnabled: true, facesInstalled: false, analysing: 0 })?.title).toBe('Not set up on this server');
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 1204 })?.title).toMatch(/^Analysing 1.204 photos…$/);
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 0 })).toBeNull();
	});
});
