import { describe, expect, it } from 'vitest';
import { hasBlurred, initialBlurState, isBlurred, withIds, withRevealAll, withRevealed } from './blurState.ts';

describe('blur state', () => {
	it('blurs only listed photos once the list arrived', () => {
		const state = withIds(initialBlurState(false), ['a', 'b']);
		expect(isBlurred(state, 'a')).toBe(true);
		expect(isBlurred(state, 'c')).toBe(false);
		expect(hasBlurred(state)).toBe(true);
	});

	it('blurs everything while loading only if the filter was on last time', () => {
		expect(isBlurred(initialBlurState(true), 'x')).toBe(true);
		expect(isBlurred(initialBlurState(false), 'x')).toBe(false);
		expect(hasBlurred(initialBlurState(true)), 'no toggle before the list').toBe(false);
	});

	it('remembers whether the filter blurred anything', () => {
		expect(withIds(initialBlurState(false), ['a']).wasActive).toBe(true);
		expect(withIds(initialBlurState(true), []).wasActive).toBe(false);
	});

	it('reveals one photo, then all, then blurs all again', () => {
		const listed = withIds(initialBlurState(false), ['a', 'b']);
		const one = withRevealed(listed, 'a');
		expect(isBlurred(one, 'a')).toBe(false);
		expect(isBlurred(one, 'b')).toBe(true);
		const all = withRevealAll(one, true);
		expect(isBlurred(all, 'b')).toBe(false);
		const again = withRevealAll(all, false);
		expect(isBlurred(again, 'a'), 'blurring again forgets single reveals').toBe(true);
		expect(listed.revealed.size, 'states are not mutated').toBe(0);
	});

	it('keeps reveals across a refreshed list', () => {
		const revealed = withRevealed(withIds(initialBlurState(false), ['a']), 'a');
		expect(isBlurred(withIds(revealed, ['a', 'b']), 'a')).toBe(false);
	});
});
