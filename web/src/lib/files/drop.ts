/** The part of `DataTransferItem` the drop zone uses (so tests can fake it). */
export interface DroppedItem {
	kind: string;
	getAsFile(): File | null;
	webkitGetAsEntry?(): { isDirectory: boolean } | null;
}

export interface DroppedFiles {
	files: File[];
	/** Folders in the drop, which are not uploaded (folder upload is not supported yet). */
	skippedFolders: number;
}

/**
 * Regular files from a drop. Must run during the drop event: the items are emptied after it.
 * Folders are recognised with `webkitGetAsEntry()`; without items, typeless empty entries
 * (how some browsers expose folders in `files`) are treated as folders.
 */
export function collectDroppedFiles(
	items: ArrayLike<DroppedItem> | null | undefined,
	fallback: ArrayLike<File>
): DroppedFiles {
	if (!items || items.length === 0) {
		const all = Array.from(fallback);
		const files = all.filter((file) => file.type !== '' || file.size > 0);
		return { files, skippedFolders: all.length - files.length };
	}
	const files: File[] = [];
	let skippedFolders = 0;
	for (const item of Array.from(items)) {
		if (item.kind !== 'file') continue;
		if (item.webkitGetAsEntry?.()?.isDirectory) {
			skippedFolders += 1;
			continue;
		}
		const file = item.getAsFile();
		if (file) files.push(file);
	}
	return { files, skippedFolders };
}

export interface DragDepth {
	/** Call on dragenter; returns whether the pointer is over the zone. */
	enter(): boolean;
	/** Call on dragleave; returns whether the pointer is still over the zone. */
	leave(): boolean;
	/** Call on drop or cancel; returns false. */
	reset(): boolean;
}

/**
 * Counts dragenter/dragleave pairs: moving over a child fires leave on the parent, so a
 * plain boolean would flicker. The zone is "over" while the count is positive.
 */
export function dragDepth(): DragDepth {
	let depth = 0;
	return {
		enter: () => (depth += 1) > 0,
		leave: () => (depth = Math.max(0, depth - 1)) > 0,
		reset: () => (depth = 0) > 0
	};
}
