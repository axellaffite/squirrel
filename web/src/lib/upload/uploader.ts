import type { components } from '#lib/api/schema.d.ts';
import { isServerRetryable, isTransient, UploadError } from '#lib/upload/errors.ts';
import { chunkRange, missingChunks } from '#lib/upload/plan.ts';
import { abortableDelay, runPool, withRetry, type RetryOptions } from '#lib/upload/retry.ts';

export { UploadError } from '#lib/upload/errors.ts';

type FileNode = components['schemas']['NodeDto'];
type UploadSession = components['schemas']['UploadSessionDto'];

export type StartResult = { kind: 'deduplicated'; node: FileNode } | { kind: 'session'; session: UploadSession };

export interface UploadTransport {
	start(
		request: { parent_id: string; name: string; size: number; sha256: string },
		signal: AbortSignal
	): Promise<StartResult>;
	putChunk(uploadId: string, index: number, body: Blob, signal: AbortSignal): Promise<void>;
	complete(uploadId: string, signal: AbortSignal): Promise<FileNode>;
	abort(uploadId: string): Promise<void>;
}

export type UploadPhase = 'hashing' | 'uploading' | 'finishing';

export interface UploadProgress {
	phase: UploadPhase;
	done: number;
	total: number;
}

export interface UploadDeps {
	transport: UploadTransport;
	hash: (file: Blob, onProgress: (hashed: number) => void, signal: AbortSignal) => Promise<string>;
	/** Backoff wait; must reject promptly when `signal` aborts. */
	sleep?: (ms: number, signal: AbortSignal) => Promise<void>;
	/** Chunks in flight at once. */
	concurrency?: number;
	/** Attempts for starting and completing on transient failures. */
	maxAttempts?: number;
	/** Attempts per chunk PUT on transient failures. */
	chunkAttempts?: number;
}

/** Cancelled after the server started storing the file: it may be saved anyway. */
export class CancelledTooLate extends Error {
	constructor() {
		super('Cancelled, but the file may have been saved already');
		this.name = 'CancelledTooLate';
	}
}

const DEFAULT_ATTEMPTS = 4;
const DEFAULT_CHUNK_ATTEMPTS = 6;

export interface UploadResult {
	node: FileNode;
	deduplicated: boolean;
}

/**
 * Hash → start (dedup or session) → send missing chunks with retries → complete.
 * A failure keeps the server session, so calling again resumes where it stopped;
 * cancelling (aborting `signal`) deletes it.
 */
export async function uploadFile(
	file: File,
	parentId: string,
	deps: UploadDeps,
	onProgress: (progress: UploadProgress) => void,
	signal: AbortSignal
): Promise<UploadResult> {
	const total = file.size;
	const retry = retryOptions(deps);
	const sha256 = await deps.hash(file, (done) => onProgress({ phase: 'hashing', done, total }), signal);
	signal.throwIfAborted();
	const request = { parent_id: parentId, name: file.name, size: total, sha256 };
	// Starting is idempotent: the server hands back the open session for the same file.
	const started = await withRetry(() => deps.transport.start(request, signal), retry, signal);
	if (started.kind === 'deduplicated') return { node: started.node, deduplicated: true };

	const { session } = started;
	let completing = false;
	try {
		// Cancelled while the start request was in flight: the session exists server-side anyway.
		signal.throwIfAborted();
		const chunkRetry = { ...retry, attempts: deps.chunkAttempts ?? DEFAULT_CHUNK_ATTEMPTS };
		await sendMissingChunks(file, session, deps, chunkRetry, onProgress, signal);
		onProgress({ phase: 'finishing', done: total, total });
		completing = true;
		// Retried only when the server answered "busy": after a lost response the file may already exist.
		const completeRetry = { ...retry, retryable: isServerRetryable };
		const node = await withRetry(() => deps.transport.complete(session.upload_id, signal), completeRetry, signal);
		return { node, deduplicated: false };
	} catch (error) {
		if (signal.aborted && (await abortSession(deps.transport, session.upload_id, completing))) {
			throw new CancelledTooLate();
		}
		throw error;
	}
}

/**
 * Deletes the server session. Returns true when the upload may have completed anyway: the
 * server is storing it (409), or it is gone after the completion request went out.
 */
async function abortSession(transport: UploadTransport, uploadId: string, completing: boolean): Promise<boolean> {
	try {
		await transport.abort(uploadId);
		return false;
	} catch (error) {
		if (!(error instanceof UploadError)) return false;
		return error.status === 409 || (completing && error.status === 404);
	}
}

function retryOptions(deps: UploadDeps): RetryOptions {
	return { attempts: deps.maxAttempts ?? DEFAULT_ATTEMPTS, sleep: deps.sleep ?? abortableDelay, retryable: isTransient };
}

async function sendMissingChunks(
	file: File,
	session: UploadSession,
	deps: UploadDeps,
	retry: RetryOptions,
	onProgress: (progress: UploadProgress) => void,
	signal: AbortSignal
): Promise<void> {
	const total = file.size;
	const missing = missingChunks(session.chunk_count, session.received_chunks);
	const pending = missing.reduce((sum, i) => sum + rangeLength(chunkRange(i, session.chunk_size, total)), 0);
	let sent = total - pending;
	onProgress({ phase: 'uploading', done: sent, total });
	const sendChunk = async (index: number, laneSignal: AbortSignal) => {
		// Exact sizes: the server rejects a chunk whose length is not the expected one.
		const [start, end] = chunkRange(index, session.chunk_size, total);
		const body = file.slice(start, end);
		await withRetry(() => deps.transport.putChunk(session.upload_id, index, body, laneSignal), retry, laneSignal);
		sent += end - start;
		// Another chunk already failed: the upload is over, so stay quiet.
		if (!laneSignal.aborted) onProgress({ phase: 'uploading', done: sent, total });
	};
	// One failing chunk aborts the others, and the pool waits for them: no request outlives the upload.
	await runPool(missing, deps.concurrency ?? 3, sendChunk, signal);
}

function rangeLength([start, end]: [number, number]): number {
	return end - start;
}
