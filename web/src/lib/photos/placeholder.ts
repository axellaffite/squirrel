/**
 * Blurred previews for photo tiles: a blurhash decoded to a 32 × 32 canvas, kept as a data URL
 * (stretched by CSS), cached per hash so scrolling back never decodes twice.
 */
import { decodeBlurhash, isBlurhash } from '#lib/photos/blurhash.ts';

/** Decoded size: enough for a smooth blur, cheap to decode (~1 ms). */
const SIZE = 32;
const CAPACITY = 500;

/** A bounded map that forgets the least recently used entry first. */
export class Lru<V> {
	readonly #max: number;
	readonly #entries = new Map<string, V>();

	constructor(max: number) {
		this.#max = max;
	}

	get size(): number {
		return this.#entries.size;
	}

	get(key: string): V | undefined {
		if (!this.#entries.has(key)) return undefined;
		const value = this.#entries.get(key) as V;
		this.#entries.delete(key);
		this.#entries.set(key, value);
		return value;
	}

	set(key: string, value: V): void {
		this.#entries.delete(key);
		this.#entries.set(key, value);
		if (this.#entries.size > this.#max) {
			const oldest = this.#entries.keys().next().value;
			if (oldest !== undefined) this.#entries.delete(oldest);
		}
	}
}

export type Render = (pixels: Uint8ClampedArray, size: number) => string | null;

export interface Placeholders {
	/** The data URL if already decoded, null if the hash is unusable, undefined if not tried yet. */
	cached(hash: string | null | undefined): string | null | undefined;
	/** Decodes (once) and returns the data URL, or null. */
	load(hash: string | null | undefined): string | null;
}

export function createPlaceholders(render: Render, capacity: number = CAPACITY): Placeholders {
	const cache = new Lru<string | null>(capacity);
	return {
		cached: (hash) => (isBlurhash(hash) ? cache.get(hash) : null),
		load(hash) {
			if (!isBlurhash(hash)) return null;
			const known = cache.get(hash);
			if (known !== undefined) return known;
			let url: string | null;
			try {
				url = render(decodeBlurhash(hash, SIZE, SIZE), SIZE);
			} catch {
				url = null;
			}
			cache.set(hash, url);
			return url;
		}
	};
}

function renderToCanvas(pixels: Uint8ClampedArray, size: number): string | null {
	const canvas = document.createElement('canvas');
	canvas.width = size;
	canvas.height = size;
	const context = canvas.getContext('2d');
	if (!context) return null;
	const image = context.createImageData(size, size);
	image.data.set(pixels);
	context.putImageData(image, 0, 0);
	return canvas.toDataURL();
}

/** The app-wide cache (browser only). */
export const placeholders: Placeholders = createPlaceholders(renderToCanvas);

/** Runs `task` when the browser is idle (or soon, where `requestIdleCallback` is missing). */
export function whenIdle(task: () => void): () => void {
	if (typeof requestIdleCallback === 'function') {
		const handle = requestIdleCallback(task);
		return () => cancelIdleCallback(handle);
	}
	const handle = setTimeout(task, 0);
	return () => clearTimeout(handle);
}
