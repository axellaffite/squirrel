import { describe, expect, it, vi } from 'vitest';
import { UploadError } from '#lib/upload/errors.ts';
import { UploadQueue, installBeforeUnloadGuard } from '#lib/upload/queue.svelte.ts';
import type { UploadTransport } from '#lib/upload/uploader.ts';

const node = (name: string) => ({ id: `n-${name}`, name }) as never;

function session(chunkCount: number) {
	return {
		kind: 'session' as const,
		session: { upload_id: 'u1', chunk_size: 4, chunk_count: chunkCount, received_chunks: [], expires_at: '' }
	};
}

function queueWith(transport: Partial<UploadTransport>) {
	const full: UploadTransport = {
		start: vi.fn(async () => session(1)),
		putChunk: vi.fn(async () => {}),
		complete: vi.fn(async (_id: string) => node('a.bin')),
		abort: vi.fn(async () => {}),
		...transport
	};
	const queue = new UploadQueue({
		transport: full,
		hash: async () => 'f'.repeat(64),
		sleep: async () => {},
		concurrency: 2
	});
	return { queue, transport: full };
}

const file = (name = 'a.bin', size = 10) => new File([new Uint8Array(size)], name);
const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

async function settled(queue: UploadQueue) {
	for (let i = 0; i < 50 && queue.hasActiveUploads; i += 1) await tick();
}

describe('UploadQueue', () => {
	it('keeps a failed upload failed when a concurrent chunk finishes late', async () => {
		let releaseSlow!: () => void;
		const { queue } = queueWith({
			start: vi.fn(async () => session(3)),
			putChunk: vi.fn(async (_id: string, index: number) => {
				if (index === 0) return new Promise<void>((resolve) => (releaseSlow = resolve));
				throw new UploadError('quota', 507, false);
			})
		});
		queue.enqueue([file()], 'p1');
		await tick();
		releaseSlow();
		await settled(queue);
		await tick();
		const [item] = queue.items;
		expect(item.status).toBe('failed');
		expect(item.error).toBe('quota');

		queue.retry(item.id);
		expect(item.status).not.toBe('failed');
	});

	it('reports whether uploads are still running', async () => {
		let release!: () => void;
		const { queue } = queueWith({
			putChunk: vi.fn(() => new Promise<void>((resolve) => (release = resolve)))
		});
		expect(queue.hasActiveUploads).toBe(false);
		queue.enqueue([file()], 'p1');
		expect(queue.hasActiveUploads).toBe(true);
		await tick();
		release();
		await settled(queue);
		expect(queue.hasActiveUploads).toBe(false);
		expect(queue.items[0].status).toBe('done');
	});

	it('announces a settled batch once, with the folders that changed', async () => {
		const { queue } = queueWith({
			complete: vi.fn(async () => node('x'))
		});
		const settledBatches: string[][] = [];
		const perFile = vi.fn();
		queue.onUploadsSettled((parents) => settledBatches.push([...parents].sort()));
		queue.onUploadFinished(perFile);
		queue.enqueue([file('a'), file('b')], 'p1');
		queue.enqueue([file('c')], 'p2');
		await settled(queue);
		await tick();
		expect(perFile).toHaveBeenCalledTimes(3);
		expect(settledBatches).toEqual([['p1', 'p2']]);
	});

	it('does not announce a batch where nothing was saved', async () => {
		const { queue } = queueWith({
			putChunk: vi.fn(async () => {
				throw new UploadError('quota', 507, false);
			})
		});
		const listener = vi.fn();
		const stop = queue.onUploadsSettled(listener);
		queue.enqueue([file()], 'p1');
		await settled(queue);
		await tick();
		expect(listener).not.toHaveBeenCalled();
		stop();
	});

	it('cancels a running upload', async () => {
		const { queue, transport } = queueWith({
			putChunk: vi.fn(
				(_id: string, _index: number, _body: Blob, signal: AbortSignal) =>
					new Promise<void>((_resolve, reject) =>
						signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')))
					)
			)
		});
		queue.enqueue([file()], 'p1');
		await tick();
		queue.cancel(queue.items[0].id);
		await settled(queue);
		expect(queue.items[0].status).toBe('cancelled');
		expect(transport.abort).toHaveBeenCalledWith('u1');
	});
});

describe('installBeforeUnloadGuard', () => {
	function fakeWindow() {
		const listeners = new Map<string, EventListener>();
		return {
			listeners,
			addEventListener: vi.fn((type: string, listener: EventListener) => listeners.set(type, listener)),
			removeEventListener: vi.fn((type: string) => listeners.delete(type))
		};
	}

	it('asks before leaving only while uploads are active, and uninstalls', () => {
		let active = false;
		const target = fakeWindow();
		const cleanup = installBeforeUnloadGuard(() => active, target);
		const event = () => ({ preventDefault: vi.fn(), returnValue: undefined as unknown }) as unknown as BeforeUnloadEvent;

		const idle = event();
		target.listeners.get('beforeunload')!(idle);
		expect(idle.preventDefault).not.toHaveBeenCalled();

		active = true;
		const busy = event();
		target.listeners.get('beforeunload')!(busy);
		expect(busy.preventDefault).toHaveBeenCalled();

		cleanup();
		expect(target.listeners.has('beforeunload')).toBe(false);
	});
});
