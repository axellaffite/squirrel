import { describe, expect, it, vi } from 'vitest';
import type { DuplicateGroup, DuplicateMember, DuplicatesPage } from '#lib/duplicates/api.ts';
import { groupCaption, reviewOrder, suggestedKeeper } from '#lib/duplicates/model.ts';
import { DuplicatesReview, type ReviewActions } from '#lib/duplicates/review.svelte.ts';

function member(
	id: string,
	{ width = 4000, height = 3000, size = 1000, taken = '2024-07-01T10:00:00', hidden = false } = {}
): DuplicateMember {
	return {
		asset: {
			id,
			name: `${id}.jpg`,
			mime: 'image/jpeg',
			kind: 'photo',
			taken_at: taken,
			taken_offset: null,
			width,
			height,
			blurhash: null,
			thumbnail: { state: 'ready', version: 1 }
		},
		size,
		camera_make: 'Google',
		camera_model: 'Pixel 8',
		hidden,
		distance: 0
	};
}

function group(id: string, members: DuplicateMember[], state: 'open' | 'resolved' = 'open'): DuplicateGroup {
	return { id, state, taken_at: '2024-07-01T10:00:00', kept_id: null, resolved_at: null, members };
}

const small = member('small', { width: 1024, height: 768, size: 300, taken: '2024-07-01T09:00:00' });
const big = member('big', { width: 1600, height: 1200, size: 900, taken: '2024-07-01T10:00:00' });
const twin = member('twin', { width: 1600, height: 1200, size: 800, taken: '2024-07-01T08:00:00' });

function actions(): ReviewActions & { keep: ReturnType<typeof vi.fn>; restore: ReturnType<typeof vi.fn> } {
	return {
		keep: vi.fn(async (id: string, asset: string) => ({
			ok: true as const,
			data: { ...group(id, [big, small]), state: 'resolved' as const, kept_id: asset }
		})),
		restore: vi.fn(async () => ({ ok: true as const, data: null }))
	};
}

function page(items: DuplicateGroup[], next: string | null = null, open = items.length): DuplicatesPage {
	return { items, next_cursor: next, open_count: open };
}

describe('suggestedKeeper', () => {
	it('prefers the largest resolution, then the largest file, then the oldest', () => {
		expect(suggestedKeeper([small, big, twin])?.asset.id).toBe('big');
		const sameSize = member('older', { width: 1600, height: 1200, size: 900, taken: '2024-06-30T10:00:00' });
		expect(suggestedKeeper([big, sameSize])?.asset.id).toBe('older');
		expect(suggestedKeeper([])).toBeNull();
	});

	it('lists the suggested keeper first, then the others by capture time', () => {
		expect(reviewOrder([small, big, twin]).map((m) => m.asset.id)).toEqual(['big', 'twin', 'small']);
	});

	it('counts photos and hidden ones', () => {
		expect(groupCaption([big, small])).toBe('2 photos');
		expect(groupCaption([big, { ...small, hidden: true }])).toBe('2 photos, 1 hidden');
	});
});

describe('DuplicatesReview', () => {
	it('shows one tile per group, its suggested keeper, with the group size', () => {
		const review = new DuplicatesReview(actions());
		review.load(page([group('g1', [small, big, twin]), group('g2', [member('x'), member('y', { size: 2000 })])], 'next', 7));
		expect(review.tiles.map((t) => t.id)).toEqual(['big', 'y']);
		expect(review.countFor('big')).toBe(3);
		expect(review.countFor('small')).toBeNull();
		expect([review.openCount, review.cursor]).toEqual([7, 'next']);
		review.append(page([group('g2', [member('x'), member('y', { size: 2000 })]), group('g3', [member('p'), member('q')])], null, 7));
		expect(review.groups.map((g) => g.id)).toEqual(['g1', 'g2', 'g3']);
		expect(review.cursor).toBeNull();
	});

	it('opens a tile on its keeper; the panel and up/down move within the group only', () => {
		const review = new DuplicatesReview(actions());
		review.load(page([group('g1', [small, big, twin]), group('g2', [member('x'), member('y')])]));
		expect(review.shown).toBeNull();
		review.open('big');
		expect(review.shown?.asset.id).toBe('big');
		review.select('small');
		expect(review.shown?.asset.id).toBe('small');
		expect(review.tile?.id, 'the viewer still steps from the group tile').toBe('big');
		review.step(-1);
		expect(review.shown?.asset.id).toBe('twin');
		review.step(-5);
		expect(review.shown?.asset.id, 'stays at the first photo').toBe('big');
		review.step(9);
		expect(review.shown?.asset.id, 'stays in the group').toBe('small');
		review.open(review.tiles[1].id);
		expect(review.group?.id, 'left/right go to the neighbouring tile, a group').toBe('g2');
		expect(review.photoIndex).toBe(0);
		review.close();
		expect(review.shown).toBeNull();
	});

	it('keeps the photo shown, not the suggested one, and moves on to the next group', async () => {
		const requests = actions();
		const review = new DuplicatesReview(requests);
		review.load(page([group('g1', [small, big]), group('g2', [twin, member('other')])]));
		review.open('big');
		review.select('small');
		const kept = await review.keepShown();
		expect(requests.keep).toHaveBeenCalledWith('g1', 'small');
		expect(kept?.kept_id).toBe('small');
		expect(review.groups.map((g) => g.id)).toEqual(['g2']);
		expect(review.group?.id).toBe('g2');
		expect(review.openCount).toBe(1);
		await review.keepShown();
		expect(review.group, 'none left: the viewer closes').toBeNull();
	});

	it('keeps the group and the error when keeping fails', async () => {
		const requests = actions();
		requests.keep.mockResolvedValueOnce({ ok: false, error: 'Not found', status: 404 });
		const review = new DuplicatesReview(requests);
		review.load(page([group('g1', [small, big])]));
		review.open('big');
		expect(await review.keepShown()).toBeNull();
		expect(review.error).toBe('Not found');
		expect(review.groups).toHaveLength(1);
		expect(review.busy).toBe(false);
	});

	it('puts an undone group back first, closed', () => {
		const review = new DuplicatesReview(actions());
		const first = group('g1', [small, big]);
		review.load(page([group('g2', [twin, member('other')])], null, 1));
		review.restoreToQueue(first);
		expect(review.groups.map((g) => g.id)).toEqual(['g1', 'g2']);
		expect(review.openCount).toBe(2);
		expect(review.shown).toBeNull();
	});

	it('restores the photo shown or all hidden photos of a resolved group', async () => {
		const requests = actions();
		const hiddenSmall = { ...small, hidden: true };
		const hiddenTwin = { ...twin, hidden: true };
		const resolved = group('g1', [big, hiddenSmall, hiddenTwin], 'resolved');
		requests.restore.mockResolvedValueOnce({ ok: true, data: group('g1', [big, small, hiddenTwin], 'resolved') });
		const review = new DuplicatesReview(requests, { keepsResolved: true });
		review.load(page([resolved]));
		review.open('big');
		review.select('small');
		expect(await review.restore('shown')).toBe(true);
		expect(requests.restore).toHaveBeenLastCalledWith('g1', 'small');
		expect(review.shown?.asset.id).toBe('small');
		expect(review.shown?.hidden).toBe(false);
		requests.restore.mockResolvedValueOnce({ ok: true, data: group('g1', [big, small, twin], 'open') });
		await review.restore('all');
		expect(requests.restore).toHaveBeenLastCalledWith('g1', null);
		expect(review.groups, 'open again: it leaves the resolved list').toEqual([]);
		expect(review.shown).toBeNull();
	});
});
