import { describe, expect, it, vi } from 'vitest';
import type { DuplicateGroup, DuplicateMember } from '#lib/duplicates/api.ts';
import { groupCaption, memberFacts, reviewOrder, stepFor, suggestedKeeper } from '#lib/duplicates/model.ts';
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

describe('suggestedKeeper', () => {
	it('prefers the largest resolution, then the largest file, then the oldest', () => {
		expect(suggestedKeeper([small, big, twin])?.asset.id).toBe('big');
		const sameSize = member('older', { width: 1600, height: 1200, size: 900, taken: '2024-06-30T10:00:00' });
		expect(suggestedKeeper([big, sameSize])?.asset.id).toBe('older');
		expect(suggestedKeeper([])).toBeNull();
	});

	it('shows the suggested keeper first, then the others by capture time', () => {
		expect(reviewOrder([small, big, twin]).map((m) => m.asset.id)).toEqual(['big', 'twin', 'small']);
	});
});

describe('wording', () => {
	it('lists the known facts of a photo', () => {
		const facts = memberFacts(big, 'en-GB').map((fact) => fact.label);
		expect(facts).toEqual(['Resolution', 'File size', 'Taken', 'Camera', 'File name']);
		expect(memberFacts(big, 'en-GB')[0].value).toBe('1600 × 1200');
	});

	it('counts photos and hidden ones', () => {
		expect(groupCaption([big, small])).toBe('2 photos');
		expect(groupCaption([big, { ...small, hidden: true }])).toBe('2 photos, 1 hidden');
	});

	it('maps arrows, Home and End to moves', () => {
		expect([stepFor('ArrowDown'), stepFor('ArrowRight'), stepFor('ArrowUp'), stepFor('ArrowLeft')]).toEqual([1, 1, -1, -1]);
		expect([stepFor('Home'), stepFor('End'), stepFor('a')]).toEqual(['first', 'last', null]);
	});
});

describe('DuplicatesReview', () => {
	it('starts on the suggested keeper and moves through the strip by click and keys', () => {
		const review = new DuplicatesReview(actions());
		review.load([group('g1', [small, big, twin])]);
		expect(review.shown?.asset.id).toBe('big');
		review.select('small');
		expect(review.shown?.asset.id).toBe('small');
		expect(review.handleKey('ArrowUp')).toBe(true);
		expect(review.shown?.asset.id).toBe('twin');
		review.handleKey('Home');
		expect(review.shown?.asset.id).toBe('big');
		review.handleKey('ArrowLeft');
		expect(review.shown?.asset.id, 'stays at the first photo').toBe('big');
		review.handleKey('End');
		expect(review.shown?.asset.id).toBe('small');
		expect(review.handleKey('x')).toBe(false);
	});

	it('keeps the photo shown, not the suggested one, and moves on to the next group', async () => {
		const requests = actions();
		const review = new DuplicatesReview(requests);
		review.load([group('g1', [small, big]), group('g2', [twin, member('other')])]);
		review.select('small');
		const kept = await review.keepShown();
		expect(requests.keep).toHaveBeenCalledWith('g1', 'small');
		expect(kept?.kept_id).toBe('small');
		expect(review.groups.map((g) => g.id)).toEqual(['g2']);
		expect(review.group?.id).toBe('g2');
		expect(review.photoIndex).toBe(0);
	});

	it('keeps the group and the error when keeping fails', async () => {
		const requests = actions();
		requests.keep.mockResolvedValueOnce({ ok: false, error: 'Not found', status: 404 });
		const review = new DuplicatesReview(requests);
		review.load([group('g1', [small, big])]);
		expect(await review.keepShown()).toBeNull();
		expect(review.error).toBe('Not found');
		expect(review.groups).toHaveLength(1);
		expect(review.busy).toBe(false);
	});

	it('pages through groups and puts an undone group back first', () => {
		const review = new DuplicatesReview(actions());
		const first = group('g1', [small, big]);
		review.load([first, group('g2', [twin, member('other')])]);
		review.moveGroup(1);
		expect(review.group?.id).toBe('g2');
		review.moveGroup(1);
		expect(review.group?.id, 'no group after the last').toBe('g2');
		review.restoreToQueue(first);
		expect(review.group?.id).toBe('g1');
		expect(review.groups).toHaveLength(2);
	});

	it('restores the photo shown or all hidden photos of a resolved group', async () => {
		const requests = actions();
		const hiddenSmall = { ...small, hidden: true };
		const hiddenTwin = { ...twin, hidden: true };
		const resolved = group('g1', [big, hiddenSmall, hiddenTwin], 'resolved');
		requests.restore.mockResolvedValueOnce({ ok: true, data: group('g1', [big, small, hiddenTwin], 'resolved') });
		const review = new DuplicatesReview(requests, { keepsResolved: true });
		review.load([resolved]);
		review.select('small');
		expect(await review.restore('shown')).toBe(true);
		expect(requests.restore).toHaveBeenLastCalledWith('g1', 'small');
		expect(review.shown?.asset.id).toBe('small');
		expect(review.shown?.hidden).toBe(false);
		requests.restore.mockResolvedValueOnce({ ok: true, data: group('g1', [big, small, twin], 'open') });
		await review.restore('all');
		expect(requests.restore).toHaveBeenLastCalledWith('g1', null);
		expect(review.groups, 'open again: it leaves the resolved list').toEqual([]);
	});
});
