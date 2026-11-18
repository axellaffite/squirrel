import { describe, expect, it, vi } from 'vitest';
import { cleanName, emptyReason, forEach, mergeTarget } from '#lib/explore/people.ts';

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

	it('runs a step for every id and reports failures', async () => {
		const step = vi.fn(async (id: string) => (id === 'b' ? { ok: false as const, error: 'nope', status: 404 } : { ok: true as const, data: null }));
		expect(await forEach(['a', 'b', 'c'], step, 2)).toEqual({ failed: ['b'], error: 'nope' });
		expect(step).toHaveBeenCalledTimes(3);
	});

	it('explains an empty people grid', () => {
		expect(emptyReason({ facesEnabled: false, facesInstalled: true, analysing: 3 })?.title).toBe('Face recognition is off');
		expect(emptyReason({ facesEnabled: true, facesInstalled: false, analysing: 0 })?.title).toBe('Not set up on this server');
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 1204 })?.title).toMatch(/^Analysing 1.204 photos…$/);
		expect(emptyReason({ facesEnabled: true, facesInstalled: true, analysing: 0 })).toBeNull();
	});
});
