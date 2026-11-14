import { describe, expect, it } from 'vitest';
import { collectDroppedFiles, dragDepth, type DroppedItem } from '#lib/files/drop.ts';

const file = (name: string, size = 3) => new File(['x'.repeat(size)], name);

function item(entry: File | 'folder' | 'string', withEntryApi = true): DroppedItem {
	if (entry === 'string') return { kind: 'string', getAsFile: () => null };
	const isDirectory = entry === 'folder';
	const asFile = isDirectory ? file('Photos', 0) : entry;
	return {
		kind: 'file',
		getAsFile: () => asFile,
		...(withEntryApi ? { webkitGetAsEntry: () => ({ isDirectory }) } : {})
	};
}

describe('collectDroppedFiles', () => {
	it('keeps files and counts folders separately', () => {
		const a = file('a.txt');
		const b = file('b.txt');
		const result = collectDroppedFiles([item(a), item('folder'), item(b), item('string')], []);
		expect(result.files).toEqual([a, b]);
		expect(result.skippedFolders).toBe(1);
	});

	it('keeps empty regular files', () => {
		const empty = file('empty.txt', 0);
		expect(collectDroppedFiles([item(empty)], []).files).toEqual([empty]);
	});

	it('trusts getAsFile when the entry API is missing', () => {
		const a = file('a.txt');
		expect(collectDroppedFiles([item(a, false)], [])).toEqual({ files: [a], skippedFolders: 0 });
	});

	it('falls back to the file list without items, dropping typeless empty entries', () => {
		const a = new File(['abc'], 'a.txt', { type: 'text/plain' });
		const folderLike = file('Photos', 0);
		expect(collectDroppedFiles(null, [a, folderLike])).toEqual({ files: [a], skippedFolders: 1 });
	});
});

describe('dragDepth', () => {
	it('stays over while nested enters and leaves are balanced', () => {
		const depth = dragDepth();
		expect(depth.enter()).toBe(true);
		expect(depth.enter()).toBe(true); // entered a child
		expect(depth.leave()).toBe(true); // left the child, still inside
		expect(depth.leave()).toBe(false); // left the zone
	});

	it('never goes below zero and resets on drop', () => {
		const depth = dragDepth();
		expect(depth.leave()).toBe(false);
		depth.enter();
		depth.enter();
		expect(depth.reset()).toBe(false);
		expect(depth.enter()).toBe(true);
		expect(depth.leave()).toBe(false);
	});
});
