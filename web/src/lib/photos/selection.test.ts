import { describe, expect, it } from 'vitest';
import { Selection } from '#lib/photos/selection.svelte.ts';

const order = ['a', 'b', 'c', 'd', 'e', 'f'];

describe('Selection', () => {
	it('toggles ids and knows when it is active', () => {
		const selection = new Selection();
		expect(selection.active).toBe(false);
		selection.toggle('b');
		expect(selection.active).toBe(true);
		expect(selection.has('b')).toBe(true);
		selection.toggle('b');
		expect(selection.size).toBe(0);
	});

	it('selects a range from the last toggled tile, in either direction, across sections', () => {
		const selection = new Selection();
		selection.toggle('b');
		selection.selectRange('e', order);
		expect([...selection.ids].sort()).toEqual(['b', 'c', 'd', 'e']);
		selection.selectRange('a', order);
		expect([...selection.ids].sort()).toEqual(['a', 'b', 'c', 'd', 'e']);
	});

	it('toggles alone when there is no anchor', () => {
		const selection = new Selection();
		selection.selectRange('c', order);
		expect([...selection.ids]).toEqual(['c']);
	});

	it('clears and forgets the anchor', () => {
		const selection = new Selection();
		selection.toggle('a');
		selection.clear();
		expect(selection.size).toBe(0);
		selection.selectRange('c', order);
		expect([...selection.ids]).toEqual(['c']);
	});

	it('drops ids that are gone', () => {
		const selection = new Selection();
		selection.toggle('a');
		selection.toggle('b');
		selection.keepOnly(new Set(['b']));
		expect([...selection.ids]).toEqual(['b']);
	});
});
