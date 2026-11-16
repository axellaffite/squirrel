/**
 * Blurhash decoding (https://blurha.sh, MIT reference algorithm), so a photo's
 * blurred colour preview can be painted while its thumbnail loads. No dependency.
 */
const DIGITS = '0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz#$%*+,-.:;=?@[]^_{|}~';

function decode83(text: string): number {
	let value = 0;
	for (const char of text) {
		const digit = DIGITS.indexOf(char);
		if (digit < 0) throw new Error('invalid blurhash');
		value = value * 83 + digit;
	}
	return value;
}

function srgbToLinear(value: number): number {
	const v = value / 255;
	return v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4;
}

function linearToSrgb(value: number): number {
	const v = Math.max(0, Math.min(1, value));
	return Math.trunc(v <= 0.0031308 ? v * 12.92 * 255 + 0.5 : (1.055 * v ** (1 / 2.4) - 0.055) * 255 + 0.5);
}

const signPow = (value: number, exp: number) => Math.sign(value) * Math.abs(value) ** exp;

/** Whether `hash` is a well-formed blurhash (length matches its component count). */
export function isBlurhash(hash: string | null | undefined): hash is string {
	if (!hash || hash.length < 6) return false;
	try {
		const size = decode83(hash[0]);
		const nx = (size % 9) + 1;
		const ny = Math.floor(size / 9) + 1;
		return hash.length === 4 + 2 * nx * ny && decode83(hash) >= 0;
	} catch {
		return false;
	}
}

/** The average colour of a blurhash as `rgb(r g b)`: instant, before any pixel is decoded. */
export function averageColor(hash: string): string {
	const value = decode83(hash.slice(2, 6));
	return `rgb(${value >> 16} ${(value >> 8) & 255} ${value & 255})`;
}

/** RGBA pixels (`width × height × 4`) of the blurred preview. */
export function decodeBlurhash(hash: string, width: number, height: number): Uint8ClampedArray {
	if (!isBlurhash(hash)) throw new Error('invalid blurhash');
	const size = decode83(hash[0]);
	const nx = (size % 9) + 1;
	const ny = Math.floor(size / 9) + 1;
	const maxValue = (decode83(hash[1]) + 1) / 166;
	const colors: [number, number, number][] = [];
	for (let i = 0; i < nx * ny; i++) {
		if (i === 0) {
			const value = decode83(hash.slice(2, 6));
			colors.push([srgbToLinear(value >> 16), srgbToLinear((value >> 8) & 255), srgbToLinear(value & 255)]);
		} else {
			const value = decode83(hash.slice(4 + i * 2, 6 + i * 2));
			const quant = [Math.floor(value / (19 * 19)), Math.floor(value / 19) % 19, value % 19];
			colors.push(quant.map((q) => signPow((q - 9) / 9, 2) * maxValue) as [number, number, number]);
		}
	}
	const pixels = new Uint8ClampedArray(width * height * 4);
	for (let y = 0; y < height; y++) {
		for (let x = 0; x < width; x++) {
			let r = 0;
			let g = 0;
			let b = 0;
			for (let j = 0; j < ny; j++) {
				for (let i = 0; i < nx; i++) {
					const basis = Math.cos((Math.PI * x * i) / width) * Math.cos((Math.PI * y * j) / height);
					const color = colors[i + j * nx];
					r += color[0] * basis;
					g += color[1] * basis;
					b += color[2] * basis;
				}
			}
			const at = 4 * (x + y * width);
			pixels[at] = linearToSrgb(r);
			pixels[at + 1] = linearToSrgb(g);
			pixels[at + 2] = linearToSrgb(b);
			pixels[at + 3] = 255;
		}
	}
	return pixels;
}
