// Tests for native-tokens.mjs. Run: node --test scripts/
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { generate } from './native-tokens.mjs';

const sample = `
:root {
	--card: #ffffff;
	--accent: #9a3fd0;
	--tint: rgb(20 20 30 / 0.045);
	--ground: linear-gradient(180deg, #e4e4e7 0%, #cfcfd3 100%);
	--accent-soft: color-mix(in srgb, var(--accent) 12%, var(--card));
	--glow: 0 8px 22px color-mix(in srgb, var(--accent) 45%, transparent);
	--shadow-card: 0 1px 2px rgb(20 20 30 / 0.04), 0 12px 32px rgb(20 20 30 / 0.08);
	--space-2: 0.5rem;
	--gutter: var(--space-2);
	--text-sm: 0.8125rem;
	--text-display-lh: 1.05;
	--track-tight: -0.035em;
	--weight-label: 500;
	--disabled-opacity: 0.45;
	--dur-fast: 160ms;
	--ease: cubic-bezier(0.22, 1, 0.36, 1);
	--font: 'Geist', sans-serif;
	color-scheme: light;
}
@media (prefers-color-scheme: dark) {
	:root:not([data-theme='light']) {
		--card: #26272b;
		--accent: #d7a2ff;
	}
}
:root[data-theme='dark'] {
	--card: #26272b;
	--accent: #d7a2ff;
}
@media (max-width: 47.99rem) {
	:root {
		--shadow-card: 0 1px 2px rgb(20 20 30 / 0.05), 0 4px 12px rgb(20 20 30 / 0.07);
	}
}
@media (min-width: 48rem) {
	:root {
		--space-2: 1rem;
	}
}
`;

test('writes the header and plain values', () => {
	const out = generate(sample);
	assert.match(out, /^\/\/ Generated from web\/src\/lib\/styles\/tokens\.css by web\/scripts\/native-tokens\.mjs\. Do not edit\./);
	assert.match(out, /val space2 = 8\.dp/);
	assert.match(out, /val gutter = 8\.dp/);
	assert.match(out, /val textSm = 13\.sp/);
	assert.match(out, /val textDisplayLh = 1\.05\.em/);
	assert.match(out, /val trackTight = \(-0\.035\)\.em/);
	assert.match(out, /val weightLabel = FontWeight\(500\)/);
	assert.match(out, /const val disabledOpacity = 0\.45f/);
	assert.match(out, /const val durFast = 160/);
	assert.match(out, /val ease = CubicBezierEasing\(0\.22f, 1f, 0\.36f, 1f\)/);
});

test('colours live in the light and dark schemes', () => {
	const out = generate(sample);
	const light = out.slice(out.indexOf('val Light'), out.indexOf('val Dark'));
	const dark = out.slice(out.indexOf('val Dark'));
	assert.match(light, /accent = Color\(0xFF9A3FD0\)/);
	assert.match(dark, /accent = Color\(0xFFD7A2FF\)/);
	assert.match(light, /tint = Color\(red = 20, green = 20, blue = 30, alpha = 11\)/);
	assert.match(light, /ground = GradientToken\(top = Color\(0xFFE4E4E7\), bottom = Color\(0xFFCFCFD3\)\)/);
});

test('resolves color-mix per scheme', () => {
	const out = generate(sample);
	const light = out.slice(out.indexOf('val Light'), out.indexOf('val Dark'));
	const dark = out.slice(out.indexOf('val Dark'));
	// 12 % of #9a3fd0 over white: 255 - 0.12 * (255 - c).
	assert.match(light, /accentSoft = Color\(0xFFF3E8F9\)/);
	// 12 % of #d7a2ff over #26272b.
	assert.match(dark, /accentSoft = Color\(0xFF3B3644\)/);
	assert.match(light, /glow = listOf\(ShadowLayer\(x = 0\.dp, y = 8\.dp, blur = 22\.dp, spread = 0\.dp, color = Color\(red = 154, green = 63, blue = 208, alpha = 115\), inset = false\)\)/);
});

test('uses the phone values of responsive tokens', () => {
	const out = generate(sample);
	assert.match(out, /shadowCard = listOf\(ShadowLayer\(x = 0\.dp, y = 1\.dp, blur = 2\.dp, spread = 0\.dp, color = Color\(red = 20, green = 20, blue = 30, alpha = 13\), inset = false\), ShadowLayer\(x = 0\.dp, y = 4\.dp, blur = 12\.dp/);
	assert.doesNotMatch(out, /space2 = 16\.dp/);
});

test('fails on an unknown construct, naming the token', () => {
	assert.throws(() => generate(':root { --odd: calc(1px + 2px); }'), /--odd/);
	assert.throws(() => generate(':root { --tilt: linear-gradient(90deg, #000 0%, #fff 100%); }'), /--tilt/);
});

test('fails when the two dark blocks disagree', () => {
	const css = `:root { --card: #fff; }
@media (prefers-color-scheme: dark) { :root:not([data-theme='light']) { --card: #000; } }
:root[data-theme='dark'] { --card: #111; }`;
	assert.throws(() => generate(css), /dark/);
});
