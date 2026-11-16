#!/usr/bin/env node
// Builds web/static/maps/world-110m-v1.svg, the land outlines behind the photo locator, from
// Natural Earth's ne_110m_land GeoJSON (public domain), downloaded by hand from
// https://www.naturalearthdata.com (or github.com/nvkelso/natural-earth-vector, geojson/).
//
//   node scripts/build-world-svg.mjs ne_110m_land.geojson > web/static/maps/world-110m-v1.svg
//
// Equirectangular projection on a 360 × 180 grid: x = lon + 180, y = 90 − lat. Points closer
// than MIN_STEP to the previous kept one are dropped, coordinates keep one decimal.
import { readFileSync } from 'node:fs';

const MIN_STEP = 0.7;
const [input] = process.argv.slice(2);
if (!input) {
	console.error('usage: node scripts/build-world-svg.mjs <ne_110m_land.geojson>');
	process.exit(1);
}

const round = (value) => Math.round(value * 10) / 10;

function ring(points) {
	const kept = [];
	for (const [lon, lat] of points) {
		const x = round(lon + 180);
		const y = round(90 - lat);
		const last = kept[kept.length - 1];
		if (last && Math.hypot(x - last[0], y - last[1]) < MIN_STEP) continue;
		kept.push([x, y]);
	}
	if (kept.length < 3) return '';
	return `M${kept.map(([x, y]) => `${x} ${y}`).join('L')}Z`;
}

const polygons = (geometry) =>
	geometry.type === 'Polygon' ? [geometry.coordinates] : geometry.type === 'MultiPolygon' ? geometry.coordinates : [];

const { features } = JSON.parse(readFileSync(input, 'utf8'));
const d = features
	.flatMap((feature) => polygons(feature.geometry))
	.flatMap((polygon) => polygon.map(ring))
	.filter(Boolean)
	.join('');

process.stdout.write(
	`<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 360 180"><!-- Natural Earth 1:110m land, public domain. Built by scripts/build-world-svg.mjs. --><path id="land" fill-rule="evenodd" d="${d}"/></svg>\n`
);
