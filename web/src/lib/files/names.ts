const MAX_BYTES = 255;
const encoder = new TextEncoder();

/** Mirrors the server's NodeName rules closely enough for instant feedback. */
export function nameProblem(raw: string): string | null {
	const name = raw.trim().normalize('NFC');
	if (name === '') return 'Enter a name.';
	if (name === '.' || name === '..') return 'This name is reserved.';
	if (name.includes('/')) return 'Names cannot contain "/".';
	if (encoder.encode(name).length > MAX_BYTES) return 'This name is too long.';
	return null;
}
