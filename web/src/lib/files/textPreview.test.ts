import { afterEach, describe, expect, it, vi } from 'vitest';
import { fetchTextPreview } from '#lib/files/textPreview.ts';

const encode = (text: string) => new TextEncoder().encode(text);

function streamOf(...parts: Uint8Array[]) {
	let cancelled = false;
	const body = new ReadableStream<Uint8Array>({
		pull(controller) {
			const next = parts.shift();
			if (next) controller.enqueue(next);
			else controller.close();
		},
		cancel() {
			cancelled = true;
		}
	});
	return { body, cancelled: () => cancelled };
}

function answer(response: Response) {
	const fetchMock = vi.fn(async (_url: string, _init?: RequestInit) => response);
	vi.stubGlobal('fetch', fetchMock);
	return fetchMock;
}

const signal = () => new AbortController().signal;

afterEach(() => vi.unstubAllGlobals());

describe('fetchTextPreview', () => {
	it('asks for the first bytes only and passes the signal', async () => {
		const fetchMock = answer(new Response('hello', { status: 206 }));
		const abort = signal();
		expect(await fetchTextPreview('/c/1', 5000, abort, 1024)).toBe('hello');
		const [url, init] = fetchMock.mock.calls[0];
		expect(url).toBe('/c/1');
		expect(new Headers(init?.headers).get('Range')).toBe('bytes=0-1023');
		expect(init?.signal).toBe(abort);
	});

	it('skips the Range header for empty files', async () => {
		const fetchMock = answer(new Response('', { status: 200 }));
		expect(await fetchTextPreview('/c/1', 0, signal(), 1024)).toBe('');
		expect(new Headers(fetchMock.mock.calls[0][1]?.headers).has('Range')).toBe(false);
	});

	it('caps what it reads when the server ignores Range', async () => {
		const stream = streamOf(encode('abcd'), encode('efgh'), encode('ijkl'));
		answer(new Response(stream.body, { status: 200 }));
		expect(await fetchTextPreview('/c/1', 12, signal(), 6)).toBe('abcdef');
		expect(stream.cancelled()).toBe(true);
	});

	it('drops a multibyte character cut in half at the end', async () => {
		const bytes = encode('héllo é');
		const cut = bytes.slice(0, bytes.length - 1);
		answer(new Response(cut, { status: 206 }));
		expect(await fetchTextPreview('/c/1', bytes.length, signal(), cut.length)).toBe('héllo ');
	});

	it('keeps replacement characters for invalid bytes inside a complete file', async () => {
		answer(new Response(new Uint8Array([0x61, 0xff, 0x62]), { status: 200 }));
		expect(await fetchTextPreview('/c/1', 3, signal(), 1024)).toBe('a�b');
	});

	it('fails on an error status', async () => {
		answer(new Response('nope', { status: 404 }));
		await expect(fetchTextPreview('/c/1', 10, signal(), 1024)).rejects.toThrow('404');
	});
});
