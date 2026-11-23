/** Duplicates review: which photo to suggest keeping, the order to show them in, and wording. */
import type { DuplicateMember } from '#lib/duplicates/api.ts';

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
 * The order a group's photos are listed in: the suggested keeper first (shown first), then the
 * others by capture time.
 */
export function reviewOrder(members: readonly DuplicateMember[]): DuplicateMember[] {
	const suggested = suggestedKeeper(members);
	if (!suggested) return [];
	const rest = members
		.filter((member) => member.asset.id !== suggested.asset.id)
		.sort((a, b) => a.asset.taken_at.localeCompare(b.asset.taken_at) || a.asset.id.localeCompare(b.asset.id));
	return [suggested, ...rest];
}

/** "3 photos" / "2 photos, 1 hidden". */
export function groupCaption(members: readonly DuplicateMember[]): string {
	const hidden = members.filter((member) => member.hidden).length;
	const photos = `${members.length} photo${members.length === 1 ? '' : 's'}`;
	return hidden > 0 ? `${photos}, ${hidden} hidden` : photos;
}
