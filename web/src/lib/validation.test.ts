import { describe, expect, it } from 'vitest';
import { validateAccount } from '#lib/validation.ts';

const valid = {
	username: 'alice',
	displayName: 'Alice',
	password: 'correct horse',
	confirmPassword: 'correct horse'
};

describe('validateAccount', () => {
	it('accepts a valid account', () => {
		expect(validateAccount(valid)).toBeNull();
	});
	it('normalizes username case before checking', () => {
		expect(validateAccount({ ...valid, username: 'Alice' })).toBeNull();
	});
	it('rejects short usernames and bad characters', () => {
		expect(validateAccount({ ...valid, username: 'al' })).toMatch(/Username/);
		expect(validateAccount({ ...valid, username: 'al ice' })).toMatch(/Username/);
	});
	it('rejects non-ASCII usernames', () => {
		expect(validateAccount({ ...valid, username: 'élise' })).toMatch(/Username/);
	});
	it('requires a display name', () => {
		expect(validateAccount({ ...valid, displayName: '   ' })).toMatch(/Display name/);
	});
	it('enforces password length and confirmation', () => {
		expect(validateAccount({ ...valid, password: 'short', confirmPassword: 'short' })).toMatch(
			/at least 10/
		);
		expect(validateAccount({ ...valid, confirmPassword: 'different!!' })).toMatch(/do not match/);
	});
});
