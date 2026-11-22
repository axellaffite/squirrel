/**
 * The `px` a lightbox asks for: the photo fitted into the viewport, in device pixels, its long
 * side rounded up to one of the server's buckets (so a few URLs cover every screen, and each
 * stays cached). The phone app computes the same (`PreviewSize.kt`, same test vectors).
 */

import { devicePixelRatio, innerHeight, innerWidth } from 'svelte/reactivity/window';

/** Long sides the server renders previews at (`squirrel_media::preview::BUCKETS`). */
export const PREVIEW_BUCKETS = [1440, 2160, 3200] as const;

/** The server's `Retry-After` when it is too busy to render a preview (503) or limits renders (429). */
export const PREVIEW_RETRY_MS = 1000;

export interface Size {
	width: number;
	height: number;
}

/** The smallest bucket at least `px` long, or the largest. */
export function bucketFor(px: number): number {
	return PREVIEW_BUCKETS.find((bucket) => bucket >= px) ?? PREVIEW_BUCKETS[PREVIEW_BUCKETS.length - 1];
}

/**
 * `photo` (its size as displayed, `null` when unknown) contained in `viewport` (CSS pixels) at
 * `dpr` device pixels per CSS pixel: the bucket of its long side on screen, never above the
 * photo's own long side.
 */
export function previewPx(photo: Size | null, viewport: Size, dpr: number): number {
	const ratio = Number.isFinite(dpr) && dpr > 0 ? dpr : 1;
	const screen = { width: Math.max(1, viewport.width) * ratio, height: Math.max(1, viewport.height) * ratio };
	if (!photo || photo.width <= 0 || photo.height <= 0) return bucketFor(Math.max(screen.width, screen.height));
	const scale = Math.min(screen.width / photo.width, screen.height / photo.height);
	const long = Math.max(photo.width, photo.height);
	return bucketFor(Math.min(Math.ceil(long * scale), long));
}

/**
 * {@link previewPx} for this window (reactive in components: it follows resizes and zoom).
 * Without a window (server rendering) the photo gets the default 1440 bucket.
 */
export function windowPreviewPx(photo: Size | null): number {
	return previewPx(
		photo,
		{ width: innerWidth.current ?? 0, height: innerHeight.current ?? 0 },
		devicePixelRatio.current ?? 1
	);
}
