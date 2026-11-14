import { hashInWorker } from '#lib/upload/hasher.ts';
import { installBeforeUnloadGuard as installGuard, UploadQueue } from '#lib/upload/queue.svelte.ts';
import { createTransport } from '#lib/upload/transport.ts';

export type { UploadItem } from '#lib/upload/queue.svelte.ts';

/** The app-wide upload queue: `uploadQueue.items` and `uploadQueue.hasActiveUploads` are reactive. */
export const uploadQueue = new UploadQueue({ transport: createTransport(), hash: hashInWorker });

/** Reactive when read in a template or effect. */
export function hasActiveUploads(): boolean {
	return uploadQueue.hasActiveUploads;
}

export const enqueue = (files: Iterable<File>, parentId: string): void => uploadQueue.enqueue(files, parentId);
export const cancel = (id: number): void => uploadQueue.cancel(id);
export const retry = (id: number): void => uploadQueue.retry(id);
export const clearFinished = (): void => uploadQueue.clearFinished();

/** Calls `listener` after each successful upload. Returns a function that removes it. */
export const onUploadFinished = (listener: (parentId: string) => void): (() => void) =>
	uploadQueue.onUploadFinished(listener);

/**
 * Calls `listener` once per settled batch with the folders that received files, e.g. to
 * refresh the folder being shown once. Returns a function that removes it.
 */
export const onUploadsSettled = (listener: (parentIds: ReadonlySet<string>) => void): (() => void) =>
	uploadQueue.onUploadsSettled(listener);

/** Warns before leaving the page while uploads are running. Returns the cleanup; mount once in the root layout. */
export function installBeforeUnloadGuard(): () => void {
	return installGuard(hasActiveUploads);
}
