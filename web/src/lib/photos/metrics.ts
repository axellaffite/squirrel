/** Reads the photo grid's sizes from the tokens, so `tokens.css` stays the source of truth. */
import { cssLength, type Metrics } from '#lib/photos/model.ts';

/** Lays out a hidden box as tall as the token: works for `calc()`, `clamp()` and anything else. */
function probe(element: HTMLElement, name: string): number {
	const box = document.createElement('div');
	box.style.cssText = `position:absolute;visibility:hidden;pointer-events:none;height:var(${name})`;
	element.appendChild(box);
	const height = box.getBoundingClientRect().height;
	box.remove();
	return height;
}

export function readMetrics(element: HTMLElement): Metrics {
	const style = getComputedStyle(element);
	const rootPx = parseFloat(getComputedStyle(document.documentElement).fontSize) || 16;
	// Plain px/rem tokens are read directly; anything else is measured.
	const length = (name: string) => cssLength(style.getPropertyValue(name), rootPx) || probe(element, name);
	return {
		rowHeight: length('--photo-row'),
		gap: length('--space-1'),
		monthHead: length('--photo-month-head'),
		dayHead: length('--photo-day-head'),
		dayGap: length('--space-3'),
		monthGap: length('--space-6')
	};
}

export const sameMetrics = (a: Metrics | null, b: Metrics): boolean =>
	a !== null && (Object.keys(b) as (keyof Metrics)[]).every((key) => a[key] === b[key]);
