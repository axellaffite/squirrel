/** Justified rows ("Google Photos" layout) and the stacked blocks the timeline virtualizes. */

export interface Tile {
	/** Width / height as displayed; anything non-finite or ≤ 0 counts as square. */
	aspect: number;
}

export interface PlacedTile {
	index: number;
	left: number;
	width: number;
}

export interface Row {
	top: number;
	height: number;
	tiles: PlacedTile[];
}

export interface LayoutOptions {
	width: number;
	targetHeight: number;
	gap: number;
}

/** Panoramas and slivers are cropped to this range so one photo never owns a row. */
const MIN_ASPECT = 0.5;
const MAX_ASPECT = 3;

function clampAspect(aspect: number): number {
	if (!Number.isFinite(aspect) || aspect <= 0) return 1;
	return Math.min(MAX_ASPECT, Math.max(MIN_ASPECT, aspect));
}

/**
 * Greedy justified layout: tiles fill a row until its height, scaled to fill the
 * width exactly, drops to `targetHeight`. The last row keeps `targetHeight` and is
 * left-aligned. Widths are rounded so each full row ends exactly at `width`.
 */
export function justify(tiles: readonly Tile[], { width, targetHeight, gap }: LayoutOptions): { rows: Row[]; height: number } {
	const rows: Row[] = [];
	let top = 0;
	let start = 0;
	let aspectSum = 0;
	const close = (end: number, height: number, fill: boolean) => {
		const placed: PlacedTile[] = [];
		let left = 0;
		for (let i = start; i < end; i++) {
			const last = i === end - 1;
			const exact = clampAspect(tiles[i].aspect) * height;
			const w = fill && last ? width - left : Math.round(exact);
			placed.push({ index: i, left, width: Math.max(1, w) });
			left += w + gap;
		}
		const rounded = Math.round(height);
		rows.push({ top, height: rounded, tiles: placed });
		top += rounded + gap;
	};
	for (let i = 0; i < tiles.length; i++) {
		aspectSum += clampAspect(tiles[i].aspect);
		const count = i - start + 1;
		const height = (width - gap * (count - 1)) / aspectSum;
		if (height <= targetHeight) {
			close(i + 1, height, true);
			start = i + 1;
			aspectSum = 0;
		}
	}
	if (start < tiles.length) close(tiles.length, targetHeight, false);
	return { rows, height: Math.max(0, top - gap) };
}

export interface Section<T> {
	key: string;
	label: string;
	items: T[];
}

export type Block<T> =
	| { kind: 'header'; key: string; label: string; top: number; height: number }
	| { kind: 'row'; key: string; top: number; height: number; items: { item: T; left: number; width: number }[] };

/** Sections (days) stacked as header + rows, with absolute tops for virtualization. */
export function stack<T extends Tile>(
	sections: readonly Section<T>[],
	options: LayoutOptions & { headerHeight: number; sectionGap: number }
): { blocks: Block<T>[]; height: number } {
	const blocks: Block<T>[] = [];
	let top = 0;
	for (const section of sections) {
		blocks.push({ kind: 'header', key: `h:${section.key}`, label: section.label, top, height: options.headerHeight });
		top += options.headerHeight;
		const { rows, height } = justify(section.items, options);
		rows.forEach((row, n) =>
			blocks.push({
				kind: 'row',
				key: `r:${section.key}:${n}`,
				top: top + row.top,
				height: row.height,
				items: row.tiles.map((t) => ({ item: section.items[t.index], left: t.left, width: t.width }))
			})
		);
		top += height + options.sectionGap;
	}
	return { blocks, height: Math.max(0, top - options.sectionGap) };
}

/** Blocks intersecting [scrollTop - overscan, scrollTop + viewport + overscan] (binary search on sorted tops). */
export function visible<T>(blocks: readonly Block<T>[], scrollTop: number, viewport: number, overscan: number): Block<T>[] {
	const from = scrollTop - overscan;
	const to = scrollTop + viewport + overscan;
	let lo = 0;
	let hi = blocks.length;
	while (lo < hi) {
		const mid = (lo + hi) >> 1;
		if (blocks[mid].top + blocks[mid].height < from) lo = mid + 1;
		else hi = mid;
	}
	const out: Block<T>[] = [];
	for (let i = lo; i < blocks.length && blocks[i].top <= to; i++) out.push(blocks[i]);
	return out;
}
