import { describe, expect, it } from 'vitest';
import { HASH_SLICE, hashBlob } from '#lib/upload/hash-core.ts';

async function referenceSha256(bytes: Uint8Array<ArrayBuffer>): Promise<string> {
	const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', bytes));
	return Array.from(digest, (byte) => byte.toString(16).padStart(2, '0')).join('');
}

describe('hashBlob', () => {
	it('matches SHA-256 across slice boundaries and reports progress', async () => {
		const bytes = new Uint8Array(HASH_SLICE * 2 + 123).map((_, i) => i % 251);
		const progress: number[] = [];
		const hash = await hashBlob(new Blob([bytes]), (done) => progress.push(done));
		expect(hash).toBe(await referenceSha256(bytes));
		expect(progress).toEqual([HASH_SLICE, HASH_SLICE * 2, bytes.length]);
	});

	it('hashes empty files', async () => {
		expect(await hashBlob(new Blob([]))).toBe(
			'e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855'
		);
	});

	it('stops when asked', async () => {
		await expect(
			hashBlob(
				new Blob([new Uint8Array(10)]),
				() => {},
				() => true
			)
		).rejects.toThrow(/cancel/i);
	});
});
