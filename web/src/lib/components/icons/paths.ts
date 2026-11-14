/**
 * Icon geometry on a 24 × 24 grid, as SVG path data only (circles and lines are written as
 * paths so every icon renders with one element type). Shapes follow Lucide; see LICENSE.
 */

const circle = (cx: number, cy: number, r: number) =>
	`M${cx - r} ${cy}a${r} ${r} 0 1 0 ${2 * r} 0a${r} ${r} 0 1 0 ${-2 * r} 0`;

export const ICONS = {
	folder: [
		'M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z'
	],
	trash: [
		'M3 6h18',
		'M19 6v14c0 1-1 2-2 2H7c-1 0-2-1-2-2V6',
		'M8 6V4c0-1 1-2 2-2h4c1 0 2 1 2 2v2',
		'M10 11v6',
		'M14 11v6'
	],
	'user-plus': [
		'M16 21v-2a4 4 0 0 0-4-4H6a4 4 0 0 0-4 4v2',
		circle(9, 7, 4),
		'M19 8v6',
		'M22 11h-6'
	],
	user: ['M19 21v-2a4 4 0 0 0-4-4H9a4 4 0 0 0-4 4v2', circle(12, 7, 4)],
	'log-out': ['M9 21H5a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h4', 'M16 17l5-5-5-5', 'M21 12H9'],
	sun: [
		circle(12, 12, 4),
		'M12 2v2',
		'M12 20v2',
		'M4.93 4.93l1.41 1.41',
		'M17.66 17.66l1.41 1.41',
		'M2 12h2',
		'M20 12h2',
		'M6.34 17.66l-1.41 1.41',
		'M19.07 4.93l-1.41 1.41'
	],
	moon: ['M12 3a6 6 0 0 0 9 9 9 9 0 1 1-9-9Z'],
	monitor: ['M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2Z', 'M8 21h8', 'M12 17v4'],
	alert: [circle(12, 12, 10), 'M12 8v4', 'M12 16h.01'],
	check: ['M20 6 9 17l-5-5'],
	x: ['M18 6 6 18', 'M6 6l12 12'],
	'chevron-down': ['M6 9l6 6 6-6'],
	'arrow-right': ['M5 12h14', 'M12 5l7 7-7 7'],
	refresh: [
		'M3 12a9 9 0 0 1 9-9 9.75 9.75 0 0 1 6.74 2.74L21 8',
		'M21 3v5h-5',
		'M21 12a9 9 0 0 1-9 9 9.75 9.75 0 0 1-6.74-2.74L3 16',
		'M8 16H3v5'
	],
	drive: [
		'M22 12H2',
		'M5.45 5.11 2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.45-6.89A2 2 0 0 0 16.76 4H7.24a2 2 0 0 0-1.79 1.11Z',
		'M6 16h.01',
		'M10 16h.01'
	]
} as const satisfies Record<string, readonly string[]>;

export type IconName = keyof typeof ICONS;
