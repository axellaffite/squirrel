import { describe, expect, it } from 'vitest';
import { nameProblem } from '#lib/files/names.ts';

describe('nameProblem', () => {
	it.each([
		['report.pdf', null],
		['  ', 'Enter a name.'],
		['.', 'This name is reserved.'],
		['..', 'This name is reserved.'],
		['a/b', 'Names cannot contain "/".'],
		['a'.repeat(256), 'This name is too long.'],
		['é'.repeat(128), 'This name is too long.']
	])('%s → %s', (name, expected) => expect(nameProblem(name)).toBe(expected));
});
