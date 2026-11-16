/** Wording of the lightbox info panel, from the photo's details. */
import type { CameraInfo, PlaceInfo } from '#lib/photos/api.ts';

/** "Paris, Île-de-France, France" (repeated parts dropped). */
export function placeLine(place: PlaceInfo): string {
	const parts = [place.name, place.admin1, place.country].filter((part): part is string => !!part);
	return parts.filter((part, index) => parts.indexOf(part) === index).join(', ');
}

/** "Google Pixel 8": the make is dropped when the model already starts with it. */
export function cameraName(camera: CameraInfo): string | null {
	const make = camera.make?.trim() ?? '';
	const model = camera.model?.trim() ?? '';
	if (!model) return make || null;
	if (!make || model.toLowerCase().startsWith(make.toLowerCase())) return model;
	return `${make} ${model}`;
}

const trimNumber = (value: number) => String(Math.round(value * 10) / 10);

/** "1/250 · ƒ/1.7 · ISO 64 · 6.9 mm" from whatever was recorded, or null. */
export function exposureLine(camera: CameraInfo): string | null {
	const parts = [
		camera.exposure_time ? camera.exposure_time : null,
		camera.f_number ? `ƒ/${trimNumber(camera.f_number)}` : null,
		camera.iso ? `ISO ${camera.iso}` : null,
		camera.focal_length_mm ? `${trimNumber(camera.focal_length_mm)} mm` : null
	].filter((part): part is string => part !== null);
	return parts.length > 0 ? parts.join(' · ') : null;
}

/** "4032 × 3024", or null when unknown. */
export function dimensions(width: number | null | undefined, height: number | null | undefined): string | null {
	return width && height ? `${width} × ${height}` : null;
}

/** Formats the browser shows natively, so the original can stand in for a missing preview. */
const DISPLAYABLE = new Set(['image/jpeg', 'image/png', 'image/gif', 'image/webp']);

export const browserCanShow = (mime: string | null | undefined): boolean => !!mime && DISPLAYABLE.has(mime);
