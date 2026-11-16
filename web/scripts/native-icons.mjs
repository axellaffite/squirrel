#!/usr/bin/env node
// Generates the phone app's icons (Kotlin) from the web's Lucide path data. No dependency.
//
//   node --experimental-strip-types scripts/native-icons.mjs           write Icons.kt
//   node --experimental-strip-types scripts/native-icons.mjs --check   fail if Icons.kt is stale
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const HEADER =
	'// Generated from web/src/lib/components/icons/paths.ts by web/scripts/native-icons.mjs. Do not edit.';

function entryName(name) {
	return name.replace(/(^|-)([a-z0-9])/g, (_, __, c) => c.toUpperCase());
}

/** Builds Icons.kt from the ICONS map (name → SVG path data on a 24 × 24 grid). */
export function generateIcons(icons) {
	const entries = Object.entries(icons).map(([name, paths]) => {
		if (!Array.isArray(paths) || paths.length === 0) throw new Error(`${name}: no path data`);
		for (const d of paths) {
			if (typeof d !== 'string' || !/^[MmLlHhVvCcSsQqTtAaZz0-9.,\s-]+$/.test(d)) {
				throw new Error(`${name}: unsupported path data "${d}"`);
			}
		}
		return `    ${entryName(name)}(listOf(${paths.map((d) => `"${d}"`).join(', ')})),`;
	});
	return [
		HEADER,
		'package app.squirrel.theme',
		'',
		'/** The web\'s icons (Lucide shapes, see web/src/lib/components/icons/LICENSE), as SVG path data. */',
		'enum class SquirrelIcon(val paths: List<String>) {',
		...entries,
		'}',
		''
	].join('\n');
}

const here = dirname(fileURLToPath(import.meta.url));
const SOURCE = resolve(here, '../src/lib/components/icons/paths.ts');
const TARGET = resolve(here, '../../mobile/theme/src/commonMain/kotlin/app/squirrel/theme/Icons.kt');

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
	const { ICONS } = await import(pathToFileURL(SOURCE).href);
	const output = generateIcons(ICONS);
	if (process.argv.includes('--check')) {
		let current = '';
		try {
			current = readFileSync(TARGET, 'utf8');
		} catch {
			/* missing counts as stale */
		}
		if (current !== output) {
			console.error('Icons.kt is stale: run `pnpm gen:native-tokens` in web/.');
			process.exit(1);
		}
		console.log('Icons.kt is up to date.');
	} else {
		writeFileSync(TARGET, output);
		console.log(`Wrote ${TARGET}`);
	}
}
