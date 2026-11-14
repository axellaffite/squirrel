import type { QueueEntry, UploadStatus } from '#lib/upload/queue-logic.ts';
import { uploadFile, type UploadDeps } from '#lib/upload/uploader.ts';

export interface UploadItem extends QueueEntry {
	id: number;
	file: File;
	parentId: string;
}

type Listener<T> = (value: T) => void;

const ACTIVE: ReadonlySet<UploadStatus> = new Set(['queued', 'hashing', 'uploading', 'finishing']);

export function isActive(item: QueueEntry): boolean {
	return ACTIVE.has(item.status);
}

function subscribe<T>(listeners: Set<Listener<T>>, listener: Listener<T>): () => void {
	listeners.add(listener);
	return () => {
		listeners.delete(listener);
	};
}

/** Uploads files one at a time; items are reactive, so components can render them directly. */
export class UploadQueue {
	items: UploadItem[] = $state([]);
	/** Something is waiting or in flight: worth warning before the page goes away. */
	readonly hasActiveUploads: boolean = $derived(this.items.some(isActive));

	readonly #deps: UploadDeps;
	readonly #controllers = new Map<number, AbortController>();
	readonly #finishedListeners = new Set<Listener<string>>();
	readonly #settledListeners = new Set<Listener<ReadonlySet<string>>>();
	/** Folders that received a file since the queue last went idle. */
	#changedParents = new Set<string>();
	#nextId = 1;
	#running = false;

	constructor(deps: UploadDeps) {
		this.#deps = deps;
	}

	/** Calls `listener` after each successful upload. Returns a function that removes it. */
	onUploadFinished(listener: Listener<string>): () => void {
		return subscribe(this.#finishedListeners, listener);
	}

	/**
	 * Calls `listener` once each time the queue goes idle, with the folders that received
	 * at least one file during that batch, so a view refreshes once instead of per file.
	 * Not called when nothing was saved. Returns a function that removes it.
	 */
	onUploadsSettled(listener: Listener<ReadonlySet<string>>): () => void {
		return subscribe(this.#settledListeners, listener);
	}

	enqueue(files: Iterable<File>, parentId: string): void {
		for (const file of files) {
			this.items.push({
				id: this.#nextId++,
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
		void this.#pump();
	}

	cancel(id: number): void {
		const item = this.#find(id);
		if (item?.status === 'queued') item.status = 'cancelled';
		this.#controllers.get(id)?.abort();
	}

	/** Resumes from the chunks the server already has. */
	retry(id: number): void {
		const item = this.#find(id);
		if (!item || (item.status !== 'failed' && item.status !== 'cancelled')) return;
		item.status = 'queued';
		item.error = null;
		void this.#pump();
	}

	clearFinished(): void {
		this.items = this.items.filter((item) => item.status !== 'done' && item.status !== 'cancelled');
	}

	#find(id: number): UploadItem | undefined {
		return this.items.find((candidate) => candidate.id === id);
	}

	async #pump(): Promise<void> {
		if (this.#running) return;
		this.#running = true;
		try {
			for (let item = this.#nextQueued(); item; item = this.#nextQueued()) await this.#run(item);
		} finally {
			this.#running = false;
			this.#settle();
		}
	}

	#nextQueued(): UploadItem | undefined {
		return this.items.find((item) => item.status === 'queued');
	}

	#settle(): void {
		const changed = this.#changedParents;
		if (changed.size === 0) return;
		this.#changedParents = new Set();
		for (const listener of this.#settledListeners) listener(changed);
	}

	async #run(item: UploadItem): Promise<void> {
		const controller = new AbortController();
		this.#controllers.set(item.id, controller);
		item.status = 'hashing';
		item.error = null;
		// Once the run is over, a straggling callback must not overwrite the final status.
		let finished = false;
		const onProgress = (progress: { phase: UploadStatus; done: number; total: number }) => {
			if (finished) return;
			item.status = progress.phase;
			item.done = progress.done;
			item.total = progress.total;
		};
		try {
			const result = await uploadFile(item.file, item.parentId, this.#deps, onProgress, controller.signal);
			finished = true;
			item.status = 'done';
			item.deduplicated = result.deduplicated;
			item.finalName = result.node.name;
			this.#changedParents.add(item.parentId);
			for (const listener of this.#finishedListeners) listener(item.parentId);
		} catch (error) {
			finished = true;
			if (controller.signal.aborted) {
				item.status = 'cancelled';
			} else {
				item.status = 'failed';
				item.error = error instanceof Error ? error.message : String(error);
			}
		} finally {
			this.#controllers.delete(item.id);
		}
	}
}

interface UnloadTarget {
	addEventListener(type: 'beforeunload', listener: (event: BeforeUnloadEvent) => void): void;
	removeEventListener(type: 'beforeunload', listener: (event: BeforeUnloadEvent) => void): void;
}

/**
 * Makes the browser ask for confirmation before leaving while `isBusy()` holds.
 * Returns a function that removes the guard (fits `onMount` / `$effect`).
 */
export function installBeforeUnloadGuard(isBusy: () => boolean, target: UnloadTarget = window): () => void {
	const onBeforeUnload = (event: BeforeUnloadEvent) => {
		if (!isBusy()) return;
		event.preventDefault();
		// Older browsers only show the prompt when returnValue is set.
		event.returnValue = '';
	};
	target.addEventListener('beforeunload', onBeforeUnload);
	return () => target.removeEventListener('beforeunload', onBeforeUnload);
}
