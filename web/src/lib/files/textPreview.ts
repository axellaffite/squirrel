/** Text previews show the start of the file only. */
export const TEXT_PREVIEW_BYTES = 256 * 1024;

/**
 * Fetches at most `maxBytes` from the start of a file and decodes them as UTF-8.
 * Asks for a byte range (except for empty files, which have none), but also caps the
 * read in case the server sends the whole file. Invalid bytes become U+FFFD; a
 * multibyte character cut by the cap is dropped instead.
 */
export async function fetchTextPreview(
	url: string,
	size: number,
	signal: AbortSignal,
	maxBytes: number = TEXT_PREVIEW_BYTES
): Promise<string> {
	const headers: HeadersInit = size > 0 ? { Range: `bytes=0-${maxBytes - 1}` } : {};
	const response = await fetch(url, { headers, credentials: 'same-origin', signal });
	if (!response.ok) throw new Error(`Preview request failed with status ${response.status}`);
	const bytes = await readAtMost(response, maxBytes);
	const decoder = new TextDecoder('utf-8', { fatal: false });
	// Streaming mode holds back an incomplete trailing sequence; not flushing drops it.
	const truncated = bytes.length < size;
	return truncated ? decoder.decode(bytes, { stream: true }) : decoder.decode(bytes);
}

async function readAtMost(response: Response, maxBytes: number): Promise<Uint8Array> {
	if (!response.body) return new Uint8Array(await response.arrayBuffer()).slice(0, maxBytes);
	const reader = response.body.getReader();
	const parts: Uint8Array[] = [];
	let length = 0;
	try {
		while (length < maxBytes) {
			const { done, value } = await reader.read();
			if (done) break;
			parts.push(value);
			length += value.length;
		}
	} finally {
		if (length >= maxBytes) await reader.cancel().catch(() => undefined);
		else reader.releaseLock();
	}
	return concat(parts, Math.min(length, maxBytes));
}

function concat(parts: readonly Uint8Array[], length: number): Uint8Array {
	const out = new Uint8Array(length);
	let offset = 0;
	for (const part of parts) {
		if (offset >= length) break;
		const take = part.subarray(0, length - offset);
		out.set(take, offset);
		offset += take.length;
	}
	return out;
}
