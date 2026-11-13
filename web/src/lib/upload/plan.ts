/** Indexes in `0..chunkCount` that the server has not received yet. */
export function missingChunks(chunkCount: number, received: readonly number[]): number[] {
	const have = new Set(received);
	return Array.from({ length: chunkCount }, (_, i) => i).filter((i) => !have.has(i));
}

/** `[start, end)` byte range of chunk `index`: every chunk is full except the last. */
export function chunkRange(index: number, chunkSize: number, size: number): [number, number] {
	const start = index * chunkSize;
	return [start, Math.min(start + chunkSize, size)];
}
