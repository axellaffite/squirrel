import { describe, expect, it } from 'vitest';
import { nodeIcon } from '#lib/files/nodeIcon.ts';

describe('nodeIcon', () => {
	it.each([
		[{ kind: 'folder', mime: null }, 'folder'],
		[{ kind: 'file', mime: 'image/png' }, 'image'],
		[{ kind: 'file', mime: 'video/mp4' }, 'film'],
		[{ kind: 'file', mime: 'audio/mpeg' }, 'music'],
		[{ kind: 'file', mime: 'application/pdf' }, 'file-text'],
		[{ kind: 'file', mime: 'text/plain' }, 'file-text'],
		[{ kind: 'file', mime: 'application/zip' }, 'file'],
		[{ kind: 'file', mime: null }, 'file']
	])('%o → %s', (node, icon) => {
		expect(nodeIcon(node)).toBe(icon);
	});
});
