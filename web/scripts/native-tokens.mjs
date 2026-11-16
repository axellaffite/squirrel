#!/usr/bin/env node
// Generates the phone app's design tokens (Kotlin, Compose Multiplatform) from tokens.css.
// No dependency. MASTER.md › "Phone app": the phone column of tokens.css, never typed in by hand.
//
//   node scripts/native-tokens.mjs           write Tokens.kt
//   node scripts/native-tokens.mjs --check   fail if Tokens.kt is stale
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HEADER =
	'// Generated from web/src/lib/styles/tokens.css by web/scripts/native-tokens.mjs. Do not edit.';

const LIGHT = ':root';
const DARK = ":root[data-theme='dark']";
const DARK_MEDIA = "@media (prefers-color-scheme: dark) > :root:not([data-theme='light'])";
const PHONE = '@media (max-width: 47.99rem)';

/** Tokens with no native meaning, left out on purpose (anything else unknown fails). */
const SKIPPED = new Set([
	'--font', // Geist is bundled as a font resource (Type.kt).
	'--font-mono',
	'--measure', // ch units: text measure is a web layout concern.
	'--preview-max', // viewport units: sized by the native layout.
	'--sheet-max',
	'--center-min'
]);

// ---------------------------------------------------------------- CSS parsing

/** Splits CSS into blocks of custom properties keyed by their nesting context. */
function parseBlocks(css) {
	const text = css.replace(/\/\*[\s\S]*?\*\//g, '');
	const blocks = [];
	const stack = [];
	let buffer = '';
	for (const ch of text) {
		if (ch === '{') {
			stack.push({ prelude: buffer.trim().replace(/\s+/g, ' '), body: '' });
			buffer = '';
		} else if (ch === '}') {
			const frame = stack.pop();
			if (!frame) throw new Error('Unbalanced braces in tokens.css');
			frame.body += buffer;
			buffer = '';
			const context = [...stack.map((f) => f.prelude), frame.prelude].join(' > ');
			blocks.push({ context, declarations: parseDeclarations(frame.body) });
		} else {
			buffer += ch;
			if (ch === ';' && stack.length) {
				stack[stack.length - 1].body += buffer;
				buffer = '';
			}
		}
	}
	return blocks;
}

function parseDeclarations(body) {
	const map = new Map();
	for (const part of body.split(';')) {
		const index = part.indexOf(':');
		if (index < 0) continue;
		const name = part.slice(0, index).trim();
		if (!name.startsWith('--')) continue;
		map.set(name, part.slice(index + 1).trim());
	}
	return map;
}

function blockAt(blocks, context) {
	const merged = new Map();
	for (const block of blocks) {
		if (block.context === context) for (const [k, v] of block.declarations) merged.set(k, v);
	}
	return merged;
}

function sameMap(a, b) {
	return a.size === b.size && [...a].every(([k, v]) => b.get(k) === v);
}

// ---------------------------------------------------------------- values

/** Splits on commas that are not inside parentheses. */
function splitTop(value, separator = ',') {
	const parts = [];
	let depth = 0;
	let current = '';
	for (const ch of value) {
		if (ch === '(') depth++;
		if (ch === ')') depth--;
		if (ch === separator && depth === 0) {
			parts.push(current.trim());
			current = '';
		} else current += ch;
	}
	if (current.trim()) parts.push(current.trim());
	return parts;
}

/** Splits on whitespace that is not inside parentheses. */
function splitSpaces(value) {
	const parts = [];
	let depth = 0;
	let current = '';
	for (const ch of value) {
		if (ch === '(') depth++;
		if (ch === ')') depth--;
		if (/\s/.test(ch) && depth === 0) {
			if (current) parts.push(current);
			current = '';
		} else current += ch;
	}
	if (current) parts.push(current);
	return parts;
}

class Resolver {
	constructor(values) {
		this.values = values;
	}

	raw(name) {
		const value = this.values.get(name);
		if (value === undefined) throw new Error(`${name} is referenced but not defined`);
		return value;
	}

	/** Replaces a whole-value var() reference. */
	deref(value) {
		let current = value;
		for (let i = 0; i < 10; i++) {
			const m = /^var\((--[\w-]+)\)$/.exec(current.trim());
			if (!m) return current.trim();
			current = this.raw(m[1]);
		}
		throw new Error(`var() cycle at ${value}`);
	}

	color(value) {
		const v = this.deref(value);
		if (v === 'transparent') return { r: 0, g: 0, b: 0, a: 0 };
		let m = /^#([0-9a-f]{6})$/i.exec(v);
		if (m) {
			const n = parseInt(m[1], 16);
			return { r: n >> 16, g: (n >> 8) & 255, b: n & 255, a: 1 };
		}
		m = /^#([0-9a-f]{3})$/i.exec(v);
		if (m) {
			const [r, g, b] = [...m[1]].map((c) => parseInt(c + c, 16));
			return { r, g, b, a: 1 };
		}
		m = /^rgb\(\s*(\d+)\s+(\d+)\s+(\d+)\s*(?:\/\s*([\d.]+))?\s*\)$/.exec(v);
		if (m) return { r: +m[1], g: +m[2], b: +m[3], a: m[4] === undefined ? 1 : +m[4] };
		m = /^color-mix\(in srgb,\s*(.+?)\s+([\d.]+)%,\s*(.+)\)$/.exec(v);
		if (m) return mix(this.color(m[1]), this.color(m[3]), +m[2] / 100);
		return null;
	}
}

/** CSS color-mix in srgb: premultiplied interpolation. */
function mix(x, y, p) {
	const a = x.a * p + y.a * (1 - p);
	if (a === 0) return { r: 0, g: 0, b: 0, a: 0 };
	const ch = (cx, cy) => (cx * x.a * p + cy * y.a * (1 - p)) / a;
	return { r: ch(x.r, y.r), g: ch(x.g, y.g), b: ch(x.b, y.b), a };
}

function hex2(n) {
	return Math.round(n).toString(16).padStart(2, '0').toUpperCase();
}

function kColor({ r, g, b, a }) {
	const alpha = Math.round(a * 255);
	if (alpha === 255) return `Color(0xFF${hex2(r)}${hex2(g)}${hex2(b)})`;
	const [R, G, B] = [r, g, b].map(Math.round);
	return `Color(red = ${R}, green = ${G}, blue = ${B}, alpha = ${alpha})`;
}

function kNumber(n) {
	const s = String(Math.round(n * 10000) / 10000);
	return n < 0 ? `(${s})` : s;
}

function kFloat(n) {
	return `${kNumber(n)}f`;
}

/** A CSS length as a Kotlin Dp, or null. */
function kDp(value) {
	const m = /^(-?[\d.]+)(rem|px)$/.exec(value);
	if (!m) return value === '0' ? '0.dp' : null;
	const px = m[2] === 'rem' ? +m[1] * 16 : +m[1];
	return `${kNumber(px)}.dp`;
}

function convertShadow(value, resolver, name) {
	const v = resolver.deref(value);
	const layers = splitTop(v).map((layer) => {
		const parts = splitSpaces(layer);
		const inset = parts[0] === 'inset';
		if (inset) parts.shift();
		const colorPart = parts.pop();
		const color = resolver.color(colorPart);
		const lengths = parts.map(kDp);
		if (!color || lengths.length < 2 || lengths.length > 4 || lengths.includes(null)) {
			throw new Error(`${name}: unsupported shadow layer "${layer}"`);
		}
		const [x, y, blur = '0.dp', spread = '0.dp'] = lengths;
		return `ShadowLayer(x = ${x}, y = ${y}, blur = ${blur}, spread = ${spread}, color = ${kColor(color)}, inset = ${inset})`;
	});
	return `listOf(${layers.join(', ')})`;
}

/**
 * Converts one token. Returns { kind: 'scheme' | 'const' | 'val', type, code }.
 * Scheme tokens (colours, gradients, shadows) differ per light/dark scheme.
 */
function convert(name, resolver) {
	const value = resolver.deref(resolver.raw(name));
	const fail = () => {
		throw new Error(`${name}: unsupported value "${value}"`);
	};

	const color = resolver.color(value);
	if (color) return { kind: 'scheme', type: 'Color', code: kColor(color) };

	let m = /^linear-gradient\(180deg,\s*(\S+)\s+0%,\s*(\S+)\s+100%\)$/.exec(value);
	if (m) {
		const [top, bottom] = [resolver.color(m[1]), resolver.color(m[2])];
		if (!top || !bottom) fail();
		return {
			kind: 'scheme',
			type: 'GradientToken',
			code: `GradientToken(top = ${kColor(top)}, bottom = ${kColor(bottom)})`
		};
	}
	if (value.startsWith('linear-gradient')) fail();

	if (/^(inset\s+)?-?[\d.]+(px|rem)?\s+-?[\d.]+(px|rem)?/.test(value)) {
		if (/(rgb|#|color-mix|var\()/.test(value)) {
			return { kind: 'scheme', type: 'List<ShadowLayer>', code: convertShadow(value, resolver, name) };
		}
	}

	m = /^(-?[\d.]+)ms$/.exec(value);
	if (m) return { kind: 'const', type: 'Int', code: kNumber(+m[1]) };

	m = /^cubic-bezier\(([^)]+)\)$/.exec(value);
	if (m) {
		const args = m[1].split(',').map((s) => kFloat(+s.trim()));
		if (args.length !== 4) fail();
		return { kind: 'val', type: 'Easing', code: `CubicBezierEasing(${args.join(', ')})` };
	}

	const isText = /^--text-/.test(name);
	m = /^(-?[\d.]+)rem$/.exec(value);
	if (m && isText) return { kind: 'val', type: 'TextUnit', code: `${kNumber(+m[1] * 16)}.sp` };
	const dp = kDp(value);
	if (dp) return { kind: 'val', type: 'Dp', code: dp };

	m = /^(-?[\d.]+)em$/.exec(value);
	if (m) return { kind: 'val', type: 'TextUnit', code: `${kNumber(+m[1])}.em` };

	m = /^(\d+)\s*\/\s*(\d+)$/.exec(value);
	if (m) return { kind: 'const', type: 'Float', code: kFloat(+m[1] / +m[2]) };

	m = /^-?[\d.]+$/.exec(value);
	if (m) {
		if (/^--weight-/.test(name)) return { kind: 'val', type: 'FontWeight', code: `FontWeight(${value})` };
		if (/-lh$/.test(name)) return { kind: 'val', type: 'TextUnit', code: `${kNumber(+value)}.em` };
		return { kind: 'const', type: 'Float', code: kFloat(+value) };
	}

	return fail();
}

function camel(name) {
	return name.replace(/^--/, '').replace(/-([a-z0-9])/g, (_, c) => c.toUpperCase());
}

// ---------------------------------------------------------------- generation

/** Builds Tokens.kt from the text of tokens.css. */
export function generate(css) {
	const blocks = parseBlocks(css);
	const base = blockAt(blocks, LIGHT);
	const dark = blockAt(blocks, DARK);
	const darkMedia = blockAt(blocks, DARK_MEDIA);
	if (darkMedia.size && !sameMap(dark, darkMedia)) {
		throw new Error('The two dark blocks of tokens.css disagree (prefers-color-scheme vs data-theme)');
	}
	const phoneLight = blockAt(blocks, `${PHONE} > ${LIGHT}`);
	const phoneDark = blockAt(blocks, `${PHONE} > ${DARK}`);

	const light = new Map([...base, ...phoneLight]);
	const darkAll = new Map([...base, ...phoneLight, ...dark, ...phoneDark]);
	const lightResolver = new Resolver(light);
	const darkResolver = new Resolver(darkAll);

	const scheme = [];
	const constants = [];
	for (const name of light.keys()) {
		if (SKIPPED.has(name)) continue;
		const l = convert(name, lightResolver);
		const d = convert(name, darkResolver);
		if (l.kind === 'scheme') {
			if (d.type !== l.type) throw new Error(`${name}: light and dark values have different types`);
			scheme.push({ name: camel(name), type: l.type, light: l.code, dark: d.code });
		} else {
			if (d.code !== l.code) throw new Error(`${name}: differs between light and dark but is not a colour`);
			constants.push({ name: camel(name), ...l });
		}
	}
	for (const name of darkAll.keys()) {
		if (!light.has(name)) throw new Error(`${name} is defined for dark only`);
	}

	const lines = [
		HEADER,
		'@file:Suppress("MagicNumber")',
		'',
		'package app.squirrel.theme',
		'',
		'import androidx.compose.animation.core.CubicBezierEasing',
		'import androidx.compose.runtime.Immutable',
		'import androidx.compose.ui.graphics.Color',
		'import androidx.compose.ui.text.font.FontWeight',
		'import androidx.compose.ui.unit.dp',
		'import androidx.compose.ui.unit.em',
		'import androidx.compose.ui.unit.sp',
		'',
		'/** The colours, gradients and shadows of one scheme (light or dark). */',
		'@Immutable',
		'class SchemeTokens(',
		...scheme.map((t) => `    val ${t.name}: ${t.type},`),
		')',
		'',
		'/** Every token of tokens.css that has a native meaning, at its phone value. */',
		'object Tokens {',
		...constants.map((t) =>
			`    ${t.kind === 'const' ? 'const val' : 'val'} ${t.name} = ${t.code}`
		),
		'',
		'    val Light = SchemeTokens(',
		...scheme.map((t) => `        ${t.name} = ${t.light},`),
		'    )',
		'',
		'    val Dark = SchemeTokens(',
		...scheme.map((t) => `        ${t.name} = ${t.dark},`),
		'    )',
		'}',
		''
	];
	return lines.join('\n');
}

// ---------------------------------------------------------------- CLI

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = resolve(here, '../src/lib/styles/tokens.css');
const TARGET = resolve(here, '../../mobile/theme/src/commonMain/kotlin/app/squirrel/theme/Tokens.kt');

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const output = generate(readFileSync(SOURCE, 'utf8'));
	if (process.argv.includes('--check')) {
		let current = '';
		try {
			current = readFileSync(TARGET, 'utf8');
		} catch {
			/* missing counts as stale */
		}
		if (current !== output) {
			console.error('Tokens.kt is stale: run `pnpm gen:native-tokens` in web/.');
			process.exit(1);
		}
		console.log('Tokens.kt is up to date.');
	} else {
		writeFileSync(TARGET, output);
		console.log(`Wrote ${TARGET}`);
	}
}
