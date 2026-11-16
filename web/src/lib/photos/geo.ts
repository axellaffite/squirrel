/** The locator's equirectangular projection and the coordinates shown next to it. */

/** Position on the 360 × 180 world map (x = lon + 180, y = 90 − lat), wrapped and clamped. */
export function project(latitude: number, longitude: number): { x: number; y: number } {
	const wrapped = ((((longitude + 180) % 360) + 360) % 360);
	const lat = Math.min(90, Math.max(-90, latitude));
	return { x: wrapped, y: 90 - lat };
}

/** "48.858400, 2.294500" (six decimals: about 0.1 m). */
export function formatCoordinates(latitude: number, longitude: number): string {
	return `${latitude.toFixed(6)}, ${longitude.toFixed(6)}`;
}

/** An OpenStreetMap link the user may choose to follow (nothing is fetched until then). */
export function openStreetMapUrl(latitude: number, longitude: number): string {
	const lat = latitude.toFixed(6);
	const lon = longitude.toFixed(6);
	return `https://www.openstreetmap.org/?mlat=${lat}&mlon=${lon}#map=15/${lat}/${lon}`;
}
