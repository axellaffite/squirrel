import { describe, expect, it, vi } from 'vitest';
import { bulk, faceTileLabels, facesCount, Generations, moveRequest, pruneSelection } from '#lib/explore/personFaces.ts';

describe('person page corrections', () => {
	it('drops selected faces that left the list', () => {
		expect([...pruneSelection(new Set(['a', 'b', 'c']), [{ id: 'a' }, { id: 'c' }])]).toEqual(['a', 'c']);
	});

	it('maps "New person" to a split and a person to an assignment', () => {
		expect(moveRequest({ newPerson: true })).toEqual({ kind: 'split' });
		expect(moveRequest({ person: { id: 'p' } })).toEqual({ kind: 'assign', person: 'p' });
	});

	it('keeps the faces that failed selected, and counts the ones done', async () => {
		const step = vi.fn(async (id: string) =>
			id === 'b' ? { ok: false as const, error: 'The server is busy.', status: 503 } : { ok: true as const, data: null }
		);
		const result = await bulk(['a', 'b', 'c'], step, 2);
		expect(result).toEqual({ done: 2, stillSelected: new Set(['b']), error: 'The server is busy.' });
		expect(facesCount(result.done)).toBe('2 faces');
		expect(facesCount(1)).toBe('1 face');
	});

	it('only the latest reload counts', () => {
		const generations = new Generations();
		const first = generations.next();
		const second = generations.next();
		expect(generations.current(first)).toBe(false);
		expect(generations.current(second)).toBe(true);
	});

	it('names every face tile differently', () => {
		const labels = faceTileLabels(
			[{ taken_at: '2024-07-14T18:30:05' }, { taken_at: '2024-07-14T18:30:40' }, { taken_at: null }],
			'en-GB'
		);
		expect(labels[0]).toMatch(/^Face from 14 July 2024 18:30$/);
		expect(labels[1]).toBe(`${labels[0]} (2)`);
		expect(labels[2]).toBe('Face 3');
	});
});
