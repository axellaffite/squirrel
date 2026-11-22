/** Duplicates review: which photo to suggest keeping, the order to show them in, and wording. */
import type { DuplicateMember } from '#lib/duplicates/api.ts';
import { formatBytes } from '#lib/files/format.ts';
import { fullDateTime } from '#lib/photos/dates.ts';
import { cameraName, dimensions } from '#lib/photos/info.ts';

const pixels = (member: DuplicateMember) => (member.asset.width ?? 0) * (member.asset.height ?? 0);

/** Better first: largest resolution, then largest file, then oldest capture, then id (stable). */
function better(a: DuplicateMember, b: DuplicateMember): number {
	return (
		pixels(b) - pixels(a) ||
		b.size - a.size ||
		a.asset.taken_at.localeCompare(b.asset.taken_at) ||
		a.asset.id.localeCompare(b.asset.id)
	);
}

/** The photo suggested to keep: largest resolution, then file size, then oldest. */
export function suggestedKeeper(members: readonly DuplicateMember[]): DuplicateMember | null {
	return [...members].sort(better)[0] ?? null;
}

/**
 * The order a group's photos are shown in: the suggested keeper first (the main preview at
 * first), then the others by capture time.
 */
export function reviewOrder(members: readonly DuplicateMember[]): DuplicateMember[] {
	const suggested = suggestedKeeper(members);
	if (!suggested) return [];
	const rest = members
		.filter((member) => member.asset.id !== suggested.asset.id)
		.sort((a, b) => a.asset.taken_at.localeCompare(b.asset.taken_at) || a.asset.id.localeCompare(b.asset.id));
	return [suggested, ...rest];
}

/** How far a key moves through a group's photos (`null`: not a move key). */
export function stepFor(key: string): number | 'first' | 'last' | null {
	switch (key) {
		case 'ArrowRight':
		case 'ArrowDown':
			return 1;
		case 'ArrowLeft':
		case 'ArrowUp':
			return -1;
		case 'Home':
			return 'first';
		case 'End':
			return 'last';
		default:
			return null;
	}
}

/** The metadata lines shown under the main preview, in order, without the unknown ones. */
export function memberFacts(member: DuplicateMember, locale?: string): { label: string; value: string }[] {
	const facts = [
		{ label: 'Resolution', value: dimensions(member.asset.width, member.asset.height) },
		{ label: 'File size', value: formatBytes(member.size) },
		{ label: 'Taken', value: fullDateTime(member.asset.taken_at, locale) },
		{ label: 'Camera', value: cameraName({ make: member.camera_make, model: member.camera_model }) },
		{ label: 'File name', value: member.asset.name }
	];
	return facts.filter((fact): fact is { label: string; value: string } => !!fact.value);
}

/** "3 photos" / "2 photos, 1 hidden". */
export function groupCaption(members: readonly DuplicateMember[]): string {
	const hidden = members.filter((member) => member.hidden).length;
	const photos = `${members.length} photo${members.length === 1 ? '' : 's'}`;
	return hidden > 0 ? `${photos}, ${hidden} hidden` : photos;
}
