import { describe, expect, it } from 'vitest';
import { EDIT_MAX_BYTES, editorUrl, isEditable, isTextMime } from './editable.ts';

const file = (mime: string | null, size = 10) => ({ kind: 'file', mime, size });

describe('isTextMime', () => {
	it.each([
		'text/plain',
		'text/markdown',
		'text/csv',
		'text/html',
		'application/json',
		'application/xml',
		'application/x-yaml',
		'application/yaml',
		'application/javascript',
		'application/toml',
		'application/x-sh',
		'application/geo+json',
		'image/svg+xml',
		'Text/Plain; charset=utf-8'
	])('accepts %s', (mime) => expect(isTextMime(mime)).toBe(true));

	it.each(['image/png', 'application/pdf', 'application/octet-stream', 'video/mp4', 'application/zip', '', null, undefined])(
		'rejects %s',
		(mime) => expect(isTextMime(mime)).toBe(false)
	);
});

describe('isEditable', () => {
	it('accepts a small text file', () => expect(isEditable(file('text/plain'))).toBe(true));

	it('accepts an empty text file', () => expect(isEditable(file('text/plain', 0))).toBe(true));

	it('accepts exactly 1 MiB and refuses one byte more', () => {
		expect(isEditable(file('text/plain', EDIT_MAX_BYTES))).toBe(true);
		expect(isEditable(file('text/plain', EDIT_MAX_BYTES + 1))).toBe(false);
	});

	it('refuses folders, binary files and files without a type', () => {
		expect(isEditable({ kind: 'folder', mime: null, size: 0 })).toBe(false);
		expect(isEditable(file('image/jpeg'))).toBe(false);
		expect(isEditable(file(null))).toBe(false);
	});
});

describe('editorUrl', () => {
	it('points at the editor route', () => expect(editorUrl('abc')).toBe('/edit/abc'));
});
