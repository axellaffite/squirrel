// Tests for native-icons.mjs. Run: node --test scripts/
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generateIcons } from './native-icons.mjs';

test('writes one enum entry per icon with its path data', () => {
	const out = generateIcons({ 'chevron-right': ['m9 18 6-6-6-6'], folder: ['M1 2Z', 'M3 4'] });
	assert.match(out, /^\/\/ Generated from web\/src\/lib\/components\/icons\/paths\.ts by web\/scripts\/native-icons\.mjs\. Do not edit\./);
	assert.match(out, /ChevronRight\(listOf\("m9 18 6-6-6-6"\)\)/);
	assert.match(out, /Folder\(listOf\("M1 2Z", "M3 4"\)\)/);
});

test('refuses path data it cannot embed', () => {
	assert.throws(() => generateIcons({ odd: ['M1 "2'] }), /odd/);
	assert.throws(() => generateIcons({ empty: [] }), /empty/);
});
