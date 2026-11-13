import { hashBlob } from './hash-core.ts';

export type HashMessage =
	| { type: 'progress'; hashed: number }
	| { type: 'done'; sha256: string }
	| { type: 'error'; message: string };

// Typed without the `webworker` lib, which clashes with the DOM lib in svelte-check.
const scope = self as unknown as {
	onmessage: ((event: MessageEvent<{ file: Blob }>) => void) | null;
	postMessage(message: HashMessage): void;
};

scope.onmessage = async (event) => {
	try {
		const sha256 = await hashBlob(event.data.file, (hashed) => scope.postMessage({ type: 'progress', hashed }));
		scope.postMessage({ type: 'done', sha256 });
	} catch (error) {
		scope.postMessage({ type: 'error', message: error instanceof Error ? error.message : String(error) });
	}
};
