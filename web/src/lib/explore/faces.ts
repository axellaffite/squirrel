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
/** Faces shorter than this share of the photo are cropped from the preview (the thumb is 360 px tall). */
export const SMALL_FACE = 0.08;

export interface CropStyle {
	/** Image height, in % of the avatar's. */
	height: number;
	/** Image top, in % of the avatar's height. */
	top: number;
	/** Horizontal shift of the image, in % of its own width (from the avatar's centre). */
	shift: number;
}

const clamp01 = (value: number) => Math.min(1, Math.max(0, value));

/** Where the photo goes inside a round avatar so `box` fills it. */
export function cropStyle(box: FaceBox, margin: number = FACE_MARGIN): CropStyle {
	const height = Math.max(box.height * (1 + 2 * margin), 0.01);
	const top = box.y - box.height * margin;
	const centre = clamp01(box.x + box.width / 2);
	return {
		height: 100 / height,
		top: (-top / height) * 100,
		shift: -centre * 100
	};
}

/** The thumbnail to crop: the preview for small faces. Null while the photo has none. */
export function cropSource(face: FaceCrop): string | null {
	if (face.thumbnail_version === null || face.thumbnail_version === undefined) return null;
	const size: ThumbnailSize = face.box.height < SMALL_FACE ? 'preview' : 'thumb';
	return thumbnailUrl(face.photo_id, size, face.thumbnail_version);
}

/** "Anna" or "Unnamed person". */
export const personLabel = (name: string | null | undefined): string => name?.trim() || 'Unnamed person';
