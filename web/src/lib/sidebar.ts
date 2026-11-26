/**
 * The sidebar's photo wash (MASTER.md › "Sidebar wash"): a built-in preset or one of the owner's
 * photos, under a scrim whose alpha is computed so muted rail ink keeps 4.5:1 on the wash's
 * lightest point. Washes are data, not tokens: radial gradients have no native meaning and the
 * phone app does not use them. The choice is remembered per device, like the theme.
 */
import { decodeBlurhash, isBlurhash } from '#lib/photos/blurhash.ts';

export type SidebarPreset = 'forest' | 'beach' | 'dusk' | 'graphite';
export type Rgb = readonly [number, number, number];
export type SidebarChoice =
	| { kind: 'preset'; preset: SidebarPreset }
	| { kind: 'photo'; id: string; blurhash: string };

interface Preset {
	label: string;
	base: string;
	/** Colour and position (x %, y %) of each soft spot over the base. */
	spots: readonly (readonly [string, number, number])[];
}

export const SIDEBAR_KEY = 'squirrel-sidebar';
export const SCRIM_FLOOR = 0.22;
/** `--rail-scrim` and `--rail-muted` (white at 82 %), mirrored here for the contrast maths. */
const SCRIM: Rgb = [12, 13, 15];
const MUTED_ALPHA = 0.82;
const TARGET = 4.5;

export const PRESETS: Readonly<Record<SidebarPreset, Preset>> = {
	forest: {
		label: 'Forest',
		base: '#55705f',
		spots: [['#c9d3c4', 22, 12], ['#8fa79a', 80, 30], ['#3e5148', 30, 70], ['#1f2a24', 75, 95]]
	},
	beach: {
		label: 'Beach',
		base: '#e2d6bd',
		spots: [['#fffaf0', 25, 10], ['#b9cbd6', 80, 25], ['#f3e7cf', 30, 65], ['#c8b38c', 70, 95]]
	},
	dusk: {
		label: 'Dusk',
		base: '#4f4263',
		spots: [['#f0c08a', 25, 15], ['#d99a7a', 78, 35], ['#2e2840', 30, 75], ['#181424', 75, 95]]
	},
	graphite: {
		label: 'Graphite',
		base: '#3a3b40',
		spots: [['#4c4d52', 30, 10], ['#2a2b2f', 70, 90]]
	}
};

export const PRESET_KEYS = Object.keys(PRESETS) as SidebarPreset[];
export const DEFAULT_SIDEBAR: Extract<SidebarChoice, { kind: 'preset' }> = { kind: 'preset', preset: 'forest' };

// ---------------------------------------------------------------- colour maths

function hexToRgb(hex: string): Rgb {
	const n = parseInt(hex.slice(1), 16);
	return [n >> 16, (n >> 8) & 255, n & 255];
}

function channel(c: number): number {
	const s = c / 255;
	return s <= 0.04045 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
}

function luminance([r, g, b]: Rgb): number {
	return 0.2126 * channel(r) + 0.7152 * channel(g) + 0.0722 * channel(b);
}

/** WCAG contrast ratio between two opaque colours. */
export function contrast(a: Rgb, b: Rgb): number {
	const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
	return (hi + 0.05) / (lo + 0.05);
}

function over(top: Rgb, alpha: number, below: Rgb): Rgb {
	return [0, 1, 2].map((i) => Math.round(top[i] * alpha + below[i] * (1 - alpha))) as unknown as Rgb;
}

/** Contrast of muted rail ink on `colour` once the scrim at `alpha` is laid over it. */
export function mutedOnScrim(colour: Rgb, alpha: number): number {
	const ground = over(SCRIM, alpha, colour);
	return contrast(over([255, 255, 255], MUTED_ALPHA, ground), ground);
}

/** The smallest scrim alpha (at least the floor) that keeps muted ink at 4.5:1 on every colour. */
export function scrimAlpha(colours: readonly Rgb[]): number {
	const lightest = colours.reduce((a, b) => (luminance(b) > luminance(a) ? b : a), colours[0] ?? SCRIM);
	for (let step = Math.round(SCRIM_FLOOR * 100); step < 100; step++) {
		if (mutedOnScrim(lightest, step / 100) >= TARGET) return step / 100;
	}
	return 0.99;
}

export function presetColors(preset: SidebarPreset): Rgb[] {
	const p = PRESETS[preset];
	return [p.base, ...p.spots.map(([c]) => c)].map(hexToRgb);
}

/** Every pixel of a small decode of the photo's blurhash. */
export function photoColors(blurhash: string, width = 8, height = 12): Rgb[] {
	const px = decodeBlurhash(blurhash, width, height);
	const out: Rgb[] = [];
	for (let i = 0; i < px.length; i += 4) out.push([px[i], px[i + 1], px[i + 2]]);
	return out;
}

/** CSS background of a preset: soft radial spots over its base colour. */
export function washBackground(preset: SidebarPreset): string {
	const p = PRESETS[preset];
	const spots = p.spots.map(([c, x, y]) => `radial-gradient(70% 55% at ${x}% ${y}%, ${c} 0%, transparent 70%)`);
	return `${spots.join(', ')}, ${p.base}`;
}

// ---------------------------------------------------------------- storage

type SidebarStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

function isPreset(value: unknown): value is SidebarPreset {
	return typeof value === 'string' && value in PRESETS;
}

export function parseSidebar(raw: string | null | undefined): SidebarChoice {
	if (!raw) return DEFAULT_SIDEBAR;
	try {
		const v: unknown = JSON.parse(raw);
		if (typeof v !== 'object' || v === null) return DEFAULT_SIDEBAR;
		const o = v as Record<string, unknown>;
		if (o.kind === 'preset' && isPreset(o.preset)) return { kind: 'preset', preset: o.preset };
		if (o.kind === 'photo' && typeof o.id === 'string' && o.id && typeof o.blurhash === 'string' && isBlurhash(o.blurhash)) {
			return { kind: 'photo', id: o.id, blurhash: o.blurhash };
		}
	} catch {
		// Not JSON: fall through to the default.
	}
	return DEFAULT_SIDEBAR;
}

/** Storage can be missing or throw (private mode, blocked site data): fall back to Forest. */
export function readSidebar(storage: SidebarStorage | null = safeLocalStorage()): SidebarChoice {
	try {
		return parseSidebar(storage?.getItem(SIDEBAR_KEY));
	} catch {
		return DEFAULT_SIDEBAR;
	}
}

export function saveSidebar(choice: SidebarChoice, storage: SidebarStorage | null = safeLocalStorage()): void {
	try {
		if (choice.kind === 'preset' && choice.preset === DEFAULT_SIDEBAR.preset) storage?.removeItem(SIDEBAR_KEY);
		else storage?.setItem(SIDEBAR_KEY, JSON.stringify(choice));
	} catch {
		// Not remembered this time; the choice still applies to the current page.
	}
}

function safeLocalStorage(): SidebarStorage | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}
