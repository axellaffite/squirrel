/** Which files open in the shared text editor (no I/O). Mirrors the server's own check. */

/** The largest file the editor opens: 1 MiB. */
export const EDIT_MAX_BYTES = 1024 * 1024;

const TEXT_APPLICATIONS = new Set([
	'application/json',
	'application/ld+json',
	'application/xml',
	'application/yaml',
	'application/x-yaml',
	'application/toml',
	'application/javascript',
	'application/x-javascript',
	'application/ecmascript',
	'application/typescript',
	'application/x-sh',
	'application/x-shellscript',
	'application/sql',
	'application/x-ndjson',
	'application/csv',
	'application/x-subrip'
]);

export interface EditableCandidate {
	kind: string;
	mime?: string | null;
	size: number;
}

/** The mime type without parameters, lower case: `Text/Plain; charset=utf-8` → `text/plain`. */
export function baseMime(mime: string): string {
	return mime.split(';', 1)[0].trim().toLowerCase();
}

export function isTextMime(mime: string | null | undefined): boolean {
	if (!mime) return false;
	const base = baseMime(mime);
	return base.startsWith('text/') || TEXT_APPLICATIONS.has(base) || base.endsWith('+json') || base.endsWith('+xml');
}

/** A text file of at most 1 MiB (the server still checks it is valid UTF-8). */
export function isEditable(node: EditableCandidate): boolean {
	return node.kind === 'file' && isTextMime(node.mime) && node.size >= 0 && node.size <= EDIT_MAX_BYTES;
}

export const editorUrl = (id: string): string => `/edit/${encodeURIComponent(id)}`;
