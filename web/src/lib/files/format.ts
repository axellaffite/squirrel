const UNITS = ['B', 'KB', 'MB', 'GB', 'TB'];

/** Human-readable size in decimal units, e.g. `1.5 MB`. */
export function formatBytes(bytes: number): string {
	if (!Number.isFinite(bytes) || bytes < 0) return '—';
	let value = bytes;
	let unit = 0;
	while (value >= 1000 && unit < UNITS.length - 1) {
		value /= 1000;
		unit += 1;
	}
	const digits = unit === 0 || value >= 100 ? 0 : 1;
	return `${value.toFixed(digits)} ${UNITS[unit]}`;
}

export type PreviewKind = 'image' | 'pdf' | 'video' | 'audio' | 'text';

// Must stay within what the server serves inline (crates/squirrel-api/src/content/policy.rs),
// except text, which the preview fetches rather than navigates to.
const IMAGES = new Set(['image/png', 'image/jpeg', 'image/gif', 'image/webp', 'image/avif', 'image/bmp']);
const VIDEOS = new Set(['video/mp4', 'video/webm', 'video/ogg']);
const AUDIO = new Set(['audio/mpeg', 'audio/ogg', 'audio/wav', 'audio/webm', 'audio/flac', 'audio/mp4']);
const ACTIVE_TEXT = new Set(['text/html', 'text/xml', 'text/javascript', 'text/ecmascript', 'text/xsl']);

/** How a file can be previewed in the browser, or null if it can only be downloaded. */
export function previewKind(mime: string | null | undefined): PreviewKind | null {
	if (!mime) return null;
	if (mime === 'application/pdf') return 'pdf';
	if (IMAGES.has(mime)) return 'image';
	if (VIDEOS.has(mime)) return 'video';
	if (AUDIO.has(mime)) return 'audio';
	if (mime === 'application/json' || (mime.startsWith('text/') && !ACTIVE_TEXT.has(mime))) return 'text';
	return null;
}

export function contentUrl(id: string, download = false): string {
	return `/api/files/${encodeURIComponent(id)}/content${download ? '?download=true' : ''}`;
}

export function storageSummary(usage: { used_bytes: number; quota_bytes?: number | null }): string {
	const used = formatBytes(usage.used_bytes);
	return usage.quota_bytes == null ? `${used} used` : `${used} of ${formatBytes(usage.quota_bytes)} used`;
}

const dateFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium', timeStyle: 'short' });

export function formatDate(iso: string): string {
	return dateFormat.format(new Date(iso));
}
