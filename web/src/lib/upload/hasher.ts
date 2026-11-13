import type { HashMessage } from './hash.worker.ts';

/** Hashes `file` in a module Web Worker so the page stays responsive. */
export function hashInWorker(file: Blob, onProgress: (hashed: number) => void, signal: AbortSignal): Promise<string> {
	return new Promise((resolve, reject) => {
		if (signal.aborted) return reject(new DOMException('Upload cancelled', 'AbortError'));
		// Vite bundles the worker as a same-origin module script: allowed by `script-src 'self'`.
		const worker = new Worker(new URL('./hash.worker.ts', import.meta.url), { type: 'module' });
		const finish = () => {
			worker.terminate();
			signal.removeEventListener('abort', onAbort);
		};
		const onAbort = () => {
			finish();
			reject(new DOMException('Upload cancelled', 'AbortError'));
		};
		signal.addEventListener('abort', onAbort, { once: true });
		worker.onmessage = (event: MessageEvent<HashMessage>) => {
			const message = event.data;
			if (message.type === 'progress') return onProgress(message.hashed);
			finish();
			if (message.type === 'done') resolve(message.sha256);
			else reject(new Error(message.message));
		};
		worker.onerror = (event) => {
			finish();
			reject(new Error(event.message || 'Could not compute the file checksum.'));
		};
		worker.postMessage({ file });
	});
}
