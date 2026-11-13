import type { QueueEntry } from '#lib/upload/queue-logic.ts';
import { hashInWorker } from '#lib/upload/hasher.ts';
import { createTransport } from '#lib/upload/transport.ts';
import { uploadFile } from '#lib/upload/uploader.ts';

export interface UploadItem extends QueueEntry {
	id: number;
	file: File;
	parentId: string;
}

export const uploadQueue: { items: UploadItem[] } = $state({ items: [] });

const transport = createTransport();
const controllers = new Map<number, AbortController>();
let nextId = 1;
let running = false;
const finishedListeners = new Set<(parentId: string) => void>();

/**
 * Calls `listener` after each successful upload, e.g. to refresh the folder being shown.
 * Returns a function that removes it.
 */
export function onUploadFinished(listener: (parentId: string) => void): () => void {
	finishedListeners.add(listener);
	return () => {
		finishedListeners.delete(listener);
	};
}

export function enqueue(files: Iterable<File>, parentId: string): void {
	for (const file of files) {
		uploadQueue.items.push({
			id: nextId++,
			file,
			parentId,
			name: file.name,
			status: 'queued',
			done: 0,
			total: file.size,
			error: null,
			deduplicated: false,
			finalName: null
		});
	}
	void pump();
}

async function pump(): Promise<void> {
	if (running) return;
	running = true;
	try {
		for (let item = nextQueued(); item; item = nextQueued()) await run(item);
	} finally {
		running = false;
	}
}

function nextQueued(): UploadItem | undefined {
	return uploadQueue.items.find((item) => item.status === 'queued');
}

async function run(item: UploadItem): Promise<void> {
	const controller = new AbortController();
	controllers.set(item.id, controller);
	item.status = 'hashing';
	item.error = null;
	try {
		const result = await uploadFile(
			item.file,
			item.parentId,
			{ transport, hash: hashInWorker },
			(progress) => {
				item.status = progress.phase;
				item.done = progress.done;
				item.total = progress.total;
			},
			controller.signal
		);
		item.status = 'done';
		item.deduplicated = result.deduplicated;
		item.finalName = result.node.name;
		for (const listener of finishedListeners) listener(item.parentId);
	} catch (error) {
		if (controller.signal.aborted) {
			item.status = 'cancelled';
		} else {
			item.status = 'failed';
			item.error = error instanceof Error ? error.message : String(error);
		}
	} finally {
		controllers.delete(item.id);
	}
}

export function cancel(id: number): void {
	const item = uploadQueue.items.find((candidate) => candidate.id === id);
	if (item?.status === 'queued') item.status = 'cancelled';
	controllers.get(id)?.abort();
}

/** Resumes from the chunks the server already has. */
export function retry(id: number): void {
	const item = uploadQueue.items.find((candidate) => candidate.id === id);
	if (!item || (item.status !== 'failed' && item.status !== 'cancelled')) return;
	item.status = 'queued';
	item.error = null;
	void pump();
}

export function clearFinished(): void {
	uploadQueue.items = uploadQueue.items.filter((item) => item.status !== 'done' && item.status !== 'cancelled');
}
