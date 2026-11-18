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
/** The thumb fits 1 080 × 360 px (squirrel-media `thumbs`): 360 px tall unless wider than 3:1. */
export const THUMB_PX = 360;
export const THUMB_MAX_WIDTH = 1080;
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
 * avatar at {@link ASSUMED_DPR}, else the preview (`aspect`: the photo's width / height, for
 * panoramas whose thumb is less than 360 px tall). Null while the photo has no thumbnail.
 */
export function cropSource(face: FaceCrop, displayPx: number, aspect: number = 1): string | null {
	if (face.thumbnail_version === null || face.thumbnail_version === undefined) return null;
	const thumbHeight = Math.min(THUMB_PX, THUMB_MAX_WIDTH / (aspect > 0 ? aspect : 1));
	const covered = cropWindow(face.box).height * thumbHeight;
	const size: ThumbnailSize = covered >= displayPx * ASSUMED_DPR ? 'thumb' : 'preview';
	return thumbnailUrl(face.photo_id, size, face.thumbnail_version);
}

/** "Anna" or "Unnamed" (MASTER). */
export const personLabel = (name: string | null | undefined): string => name?.trim() || 'Unnamed';

/**
 * Faces in reading order: rows top to bottom (faces whose centres are within half a face
 * height share a row), then left to right.
 */
export function readingOrder<T extends { box: FaceBox }>(faces: readonly T[]): T[] {
	const centre = (face: T) => face.box.y + face.box.height / 2;
	const sorted = [...faces].sort((a, b) => centre(a) - centre(b));
	const rows: T[][] = [];
	for (const face of sorted) {
		const row = rows[rows.length - 1];
		if (row && Math.abs(centre(face) - centre(row[0])) <= row[0].box.height / 2) row.push(face);
		else rows.push([face]);
	}
	return rows.flatMap((row) => row.sort((a, b) => a.box.x - b.box.x));
}

/** "Face of Anna" or "Unnamed face": a face box's accessible name. */
export const faceName = (name: string | null | undefined): string => (name?.trim() ? `Face of ${name.trim()}` : 'Unnamed face');
