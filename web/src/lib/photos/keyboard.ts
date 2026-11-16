/** Arrow-key moves between the rendered tiles of the justified grid. */

export interface TileRect {
	left: number;
	top: number;
	width: number;
	height: number;
}

export type ArrowKey = 'ArrowLeft' | 'ArrowRight' | 'ArrowUp' | 'ArrowDown';

/** Tiles whose tops differ by less than this (px) share a row. */
const SAME_ROW = 1;

export function isArrowKey(key: string): key is ArrowKey {
	return key === 'ArrowLeft' || key === 'ArrowRight' || key === 'ArrowUp' || key === 'ArrowDown';
}

const centre = (rect: TileRect) => rect.left + rect.width / 2;

/** The row above or below `from`: the nearest distinct top in that direction. */
function rowTop(rects: readonly TileRect[], from: TileRect, down: boolean): number | null {
	let best: number | null = null;
	for (const rect of rects) {
		const ahead = down ? rect.top > from.top + SAME_ROW : rect.top < from.top - SAME_ROW;
		if (ahead && (best === null || (down ? rect.top < best : rect.top > best))) best = rect.top;
	}
	return best;
}

/**
 * The tile to focus after pressing `key` on tile `from` (rects in DOM order, which is reading
 * order), or -1 to stay. Left/right step through the order; up/down go to the tile of the
 * neighbouring row whose centre is closest.
 */
export function neighbour(rects: readonly TileRect[], from: number, key: ArrowKey): number {
	const current = rects[from];
	if (!current) return -1;
	if (key === 'ArrowLeft') return from > 0 ? from - 1 : -1;
	if (key === 'ArrowRight') return from < rects.length - 1 ? from + 1 : -1;
	const top = rowTop(rects, current, key === 'ArrowDown');
	if (top === null) return -1;
	let best = -1;
	let distance = Infinity;
	rects.forEach((rect, index) => {
		if (Math.abs(rect.top - top) > SAME_ROW) return;
		const d = Math.abs(centre(rect) - centre(current));
		if (d < distance) {
			distance = d;
			best = index;
		}
	});
	return best;
}
