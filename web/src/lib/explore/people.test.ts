import { describe, expect, it } from 'vitest';
import { cleanName, emptyReason, mergeTarget } from '#lib/explore/people.ts';

const person = (id: string, name: string | null, face_count: number) => ({ id, name, face_count });

describe('people', () => {
	it('keeps a named person in a merge, else the one with most faces', () => {
		expect(mergeTarget([person('a', null, 9), person('b', 'Anna', 2), person('c', 'Ben', 5)])?.id).toBe('c');
		expect(mergeTarget([person('a', null, 3), person('b', null, 7)])?.id).toBe('b');
		expect(mergeTarget([person('a', null, 3), person('b', ' ', 3)])?.id).toBe('a');
		expect(mergeTarget([])).toBeNull();
	});

	it('cleans names', () => {
		expect(cleanName('  Anna ')).toBe('Anna');
		expect(cleanName('   ')).toBeNull();
		expect(cleanName('é'.repeat(101))).toBeNull();
	});

	it('explains an empty people grid', () => {
		expect(emptyReason({ facesEnabled: false, facesInstalled: true, analysing: 3 })?.title).toBe('Face recognition is off');
		expect(emptyReason({ facesEnabled: true, facesInstalled: false, analysing: 0 })?.title).toBe('Not set up on this server');
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 1204 })?.title).toMatch(/^Analysing 1.204 photos…$/);
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 0 })).toBeNull();
	});
});
