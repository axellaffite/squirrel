/**
 * Face avatars are client-side crops of a photo's thumbnail (the server stores no face images):
 * the image is scaled so the face box (with some room around it) fills the circle's height, and
 * moved so the face is centred.
 */
import type { components } from '#lib/api/schema.d.ts';
import { thumbnailUrl, type ThumbnailSize } from '#lib/photos/api.ts';

export type FaceCrop = components['schemas']['FaceCrop'];
export type FaceBox = components['schemas']['FaceBoxDto'];

/** Room kept around the face, as a share of its height on each side. */
export const FACE_MARGIN = 0.25;
/** Thumbnail height, px (the preview is 4 × taller). */
export const THUMB_PX = 360;
/** Screens are assumed this dense when choosing between the thumb and the preview. */
export const ASSUMED_DPR = 2;

export interface CropStyle {
	/** Image height, in % of the avatar's. */
	height: number;
	/** Image top, in % of the avatar's height. */
	top: number;
	/** Horizontal shift of the image, in % of its own width (from the avatar's centre). */
	shift: number;
}

const clamp = (value: number, low: number, high: number) => Math.min(high, Math.max(low, value));

/** The part of the photo a square avatar shows (relative 0–1, inside the photo). */
export function cropWindow(
	box: FaceBox,
	aspect: number = 1,
	margin: number = FACE_MARGIN
): { top: number; height: number; centre: number; width: number } {
	const height = clamp(box.height * (1 + 2 * margin), 0.01, 1);
	const top = clamp(box.y - box.height * margin, 0, 1 - height);
	const width = clamp(height / (aspect > 0 ? aspect : 1), 0, 1);
	const centre = clamp(box.x + box.width / 2, width / 2, 1 - width / 2);
	return { top, height, centre, width };
}

/**
 * Where the photo goes inside a round avatar so the face (with room around it) fills it. The
 * window never leaves the photo (`aspect` is its width / height, once known).
 */
export function cropStyle(box: FaceBox, aspect: number = 1, margin: number = FACE_MARGIN): CropStyle {
	const crop = cropWindow(box, aspect, margin);
	return { height: 100 / crop.height, top: (-crop.top / crop.height) * 100, shift: -crop.centre * 100 };
}

/**
 * The thumbnail to crop for an avatar `displayPx` wide: the thumb when its pixels cover the
 * avatar at {@link ASSUMED_DPR}, else the preview. Null while the photo has no thumbnail.
 */
export function cropSource(face: FaceCrop, displayPx: number): string | null {
	if (face.thumbnail_version === null || face.thumbnail_version === undefined) return null;
	const covered = cropWindow(face.box).height * THUMB_PX;
	const size: ThumbnailSize = covered >= displayPx * ASSUMED_DPR ? 'thumb' : 'preview';
	return thumbnailUrl(face.photo_id, size, face.thumbnail_version);
}

/** "Anna" or "Unnamed" (MASTER). */
export const personLabel = (name: string | null | undefined): string => name?.trim() || 'Unnamed';
