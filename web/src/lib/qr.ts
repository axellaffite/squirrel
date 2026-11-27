import { encode } from 'uqr';

/** Light modules around the code: the four the QR standard asks scanners to expect. */
export const QUIET_ZONE = 4;

export interface QrShape {
	/** Side of the square, in modules, quiet zone included. */
	size: number;
	/** SVG path of the dark modules, one unit per module. */
	path: string;
}

/** The QR code of `text` (medium error correction), as a path to draw in a `size` × `size` view box. */
export function qrShape(text: string): QrShape {
	const { size, data } = encode(text, { ecc: 'M', border: QUIET_ZONE });
	const runs: string[] = [];
	data.forEach((row, y) => {
		let start = -1;
		for (let x = 0; x <= row.length; x += 1) {
			const dark = x < row.length && row[x];
			if (dark && start < 0) start = x;
			if (!dark && start >= 0) {
				const length = x - start;
				runs.push(`M${start} ${y}h${length}v1h-${length}z`);
				start = -1;
			}
		}
	});
	return { size, path: runs.join('') };
}
