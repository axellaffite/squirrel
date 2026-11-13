import { createSHA256 } from 'hash-wasm';

/** Bytes read per step: memory stays flat whatever the file size. */
export const HASH_SLICE = 4 * 1024 * 1024;

/** SHA-256 of a Blob as lowercase hex (the server's ContentHash format). */
export async function hashBlob(
	blob: Blob,
	onProgress: (hashedBytes: number) => void = () => {},
	shouldStop: () => boolean = () => false
): Promise<string> {
	const hasher = await createSHA256();
	hasher.init();
	for (let offset = 0; offset < blob.size; offset += HASH_SLICE) {
		if (shouldStop()) throw new DOMException('Hashing was cancelled', 'AbortError');
		const end = Math.min(offset + HASH_SLICE, blob.size);
		hasher.update(new Uint8Array(await blob.slice(offset, end).arrayBuffer()));
		onProgress(end);
	}
	return hasher.digest('hex');
}
