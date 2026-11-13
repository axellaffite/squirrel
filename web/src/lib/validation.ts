/** Mirrors the server rules in `squirrel-core`; the server stays authoritative. */
export const USERNAME_PATTERN = /^[a-z0-9][a-z0-9._-]{2,31}$/;
export const PASSWORD_MIN_LENGTH = 10;
export const DISPLAY_NAME_MAX_LENGTH = 64;

export interface AccountFields {
	username: string;
	displayName: string;
	password: string;
	confirmPassword: string;
}

/** Returns a user-facing error message, or null when the fields are valid. */
export function validateAccount(fields: AccountFields): string | null {
	const username = fields.username.trim().toLowerCase();
	if (!USERNAME_PATTERN.test(username)) {
		return 'Username must be 3–32 characters: lowercase letters a–z, digits, ".", "_" or "-", starting with a letter or digit.';
	}
	const displayName = fields.displayName.trim();
	if (displayName.length === 0 || [...displayName].length > DISPLAY_NAME_MAX_LENGTH) {
		return `Display name must be between 1 and ${DISPLAY_NAME_MAX_LENGTH} characters.`;
	}
	if ([...fields.password].length < PASSWORD_MIN_LENGTH) {
		return `Password must be at least ${PASSWORD_MIN_LENGTH} characters.`;
	}
	if (fields.password !== fields.confirmPassword) {
		return 'Passwords do not match.';
	}
	return null;
}
