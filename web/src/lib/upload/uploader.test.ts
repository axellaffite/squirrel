import { describe, expect, it, vi } from 'vitest';
import { UploadError, uploadFile, type UploadDeps, type UploadTransport } from '#lib/upload/uploader.ts';

const node = { id: 'n1', name: 'a.bin' } as never;

function session(chunkCount: number, received: number[] = []) {
	return {
		kind: 'session' as const,
		session: { upload_id: 'u1', chunk_size: 4, chunk_count: chunkCount, received_chunks: received, expires_at: '' }
	};
}

function deps(transport: Partial<UploadTransport>): UploadDeps & { transport: UploadTransport } {
	return {
		transport: {
			start: vi.fn(async () => session(3)),
			putChunk: vi.fn(async () => {}),
			complete: vi.fn(async () => node),
			abort: vi.fn(async () => {}),
			...transport
		},
		hash: vi.fn(async () => 'f'.repeat(64)),
		sleep: async () => {},
		concurrency: 2
	};
}

const file = () => new File([new Uint8Array(10)], 'a.bin');
const run = (d: UploadDeps, signal = new AbortController().signal) => uploadFile(file(), 'p1', d, () => {}, signal);
const busy = () => new UploadError('busy', 503, true);

describe('uploadFile', () => {
	it('links known content without sending chunks', async () => {
		const d = deps({ start: vi.fn(async () => ({ kind: 'deduplicated' as const, node })) });
		expect(await run(d)).toEqual({ node, deduplicated: true });
		expect(d.transport.putChunk).not.toHaveBeenCalled();
	});

	it('sends only the missing chunks with the right sizes, then completes', async () => {
		const d = deps({ start: vi.fn(async () => session(3, [1])) });
		const progress: number[] = [];
		await uploadFile(file(), 'p1', d, (p) => progress.push(p.done), new AbortController().signal);
		const calls = vi.mocked(d.transport.putChunk).mock.calls.map(([, index, body]) => [index, body.size]);
		expect(calls.sort()).toEqual([
			[0, 4],
			[2, 2]
		]);
		expect(d.transport.complete).toHaveBeenCalledWith('u1', expect.anything());
		expect(progress.at(-1)).toBe(10);
	});

	it('retries transient chunk failures', async () => {
		let failures = 1;
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				if (failures-- > 0) throw busy();
			})
		});
		await run(d);
		expect(d.transport.putChunk).toHaveBeenCalledTimes(2);
	});

	it('retries starting and completing while the server is busy', async () => {
		let startFailures = 1;
		let completeFailures = 1;
		const d = deps({
			start: vi.fn(async () => {
				if (startFailures-- > 0) throw busy();
				return session(1);
			}),
			complete: vi.fn(async () => {
				if (completeFailures-- > 0) throw busy();
				return node;
			})
		});
		await run(d);
		expect(d.transport.start).toHaveBeenCalledTimes(2);
		expect(d.transport.complete).toHaveBeenCalledTimes(2);
	});

	it('does not retry completing after a network failure: the file may already exist', async () => {
		const d = deps({
			start: vi.fn(async () => session(1)),
			complete: vi.fn(async () => {
				throw new TypeError('network');
			})
		});
		await expect(run(d)).rejects.toThrow('network');
		expect(d.transport.complete).toHaveBeenCalledTimes(1);
	});

	it('does not retry client errors and keeps the session for a later retry', async () => {
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				throw new UploadError('quota', 507, false);
			})
		});
		await expect(run(d)).rejects.toThrow('quota');
		expect(d.transport.putChunk).toHaveBeenCalledTimes(1);
		expect(d.transport.abort).not.toHaveBeenCalled();
	});

	it('aborts the server session when cancelled', async () => {
		const controller = new AbortController();
		const d = deps({
			start: vi.fn(async () => session(1)),
			putChunk: vi.fn(async () => {
				controller.abort();
				throw new DOMException('aborted', 'AbortError');
			})
		});
		await expect(run(d, controller.signal)).rejects.toThrow();
		expect(d.transport.abort).toHaveBeenCalledWith('u1');
	});
});
