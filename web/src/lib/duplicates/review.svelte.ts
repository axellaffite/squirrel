/**
 * The duplicates review: a queue of groups, the group shown, and the photo shown in it (the main
 * preview). Requests are injected, so the page and the tests drive the same logic.
 */
import type { DuplicateGroup, DuplicateMember } from '#lib/duplicates/api.ts';
import { reviewOrder, stepFor } from '#lib/duplicates/model.ts';
import type { Outcome } from '#lib/files/pages.ts';

export interface ReviewActions {
	keep: (group: string, asset: string) => Promise<Outcome<DuplicateGroup>>;
	restore: (group: string, asset: string | null) => Promise<Outcome<DuplicateGroup | null>>;
}

export class DuplicatesReview {
	groups: DuplicateGroup[] = $state.raw([]);
	/** The group shown, as an index in `groups`. */
	groupIndex = $state(0);
	/** The photo shown (main preview), as an index in the group's review order. */
	photoIndex = $state(0);
	busy = $state(false);
	error: string | null = $state(null);

	readonly #actions: ReviewActions;
	/** Open groups leave the queue once a photo is kept; resolved ones stay to be restored. */
	readonly #keepsResolved: boolean;

	constructor(actions: ReviewActions, options: { keepsResolved?: boolean } = {}) {
		this.#actions = actions;
		this.#keepsResolved = options.keepsResolved ?? false;
	}

	load(groups: readonly DuplicateGroup[]): void {
		this.groups = [...groups];
		this.groupIndex = 0;
		this.photoIndex = 0;
	}

	get group(): DuplicateGroup | null {
		return this.groups[this.groupIndex] ?? null;
	}

	/** The group's photos in review order: the suggested keeper first. */
	get photos(): DuplicateMember[] {
		return this.group ? reviewOrder(this.group.members) : [];
	}

	/** The photo in the main preview. */
	get shown(): DuplicateMember | null {
		return this.photos[this.photoIndex] ?? null;
	}

	/** Shows the photo `id` of the group (a click in the strip). */
	select(id: string): void {
		const index = this.photos.findIndex((photo) => photo.asset.id === id);
		if (index >= 0) this.photoIndex = index;
	}

	/** Moves through the group's photos for `key` (arrows, Home, End). Whether the key was used. */
	handleKey(key: string): boolean {
		const step = stepFor(key);
		const count = this.photos.length;
		if (step === null || count === 0) return false;
		if (step === 'first') this.photoIndex = 0;
		else if (step === 'last') this.photoIndex = count - 1;
		else this.photoIndex = Math.min(count - 1, Math.max(0, this.photoIndex + step));
		return true;
	}

	/** Shows the group `delta` places away in the queue (its suggested keeper first). */
	moveGroup(delta: number): void {
		const next = this.groupIndex + delta;
		if (next < 0 || next >= this.groups.length) return;
		this.groupIndex = next;
		this.photoIndex = 0;
	}

	/** Keeps the photo shown; the group's others are hidden. Returns the resolved group. */
	async keepShown(): Promise<DuplicateGroup | null> {
		const group = this.group;
		const shown = this.shown;
		if (!group || !shown || this.busy) return null;
		const outcome = await this.#run(() => this.#actions.keep(group.id, shown.asset.id));
		if (!outcome) return null;
		if (this.#keepsResolved) this.#replace(group.id, outcome);
		else this.#drop(group.id);
		return outcome;
	}

	/** Shows hidden photos of the group shown again: the photo shown only, or all of them. */
	async restore(which: 'shown' | 'all'): Promise<boolean> {
		const group = this.group;
		const shown = this.shown;
		if (!group || this.busy || (which === 'shown' && !shown)) return false;
		const asset = which === 'shown' && shown ? shown.asset.id : null;
		const outcome = await this.#run(() => this.#actions.restore(group.id, asset));
		if (outcome === undefined) return false;
		// Open again (nothing hidden) or gone: it leaves the resolved list.
		if (outcome === null || outcome.state === 'open') this.#drop(group.id);
		else this.#replace(group.id, outcome);
		return true;
	}

	/** Puts a group back at the front of the queue (undoing a keep). */
	restoreToQueue(group: DuplicateGroup): void {
		this.groups = [group, ...this.groups.filter((g) => g.id !== group.id)];
		this.groupIndex = 0;
		this.photoIndex = 0;
	}

	async #run<T>(request: () => Promise<Outcome<T>>): Promise<T | undefined> {
		this.busy = true;
		this.error = null;
		try {
			const outcome = await request();
			if (!outcome.ok) {
				this.error = outcome.error;
				return undefined;
			}
			return outcome.data;
		} finally {
			this.busy = false;
		}
	}

	#drop(id: string): void {
		this.groups = this.groups.filter((group) => group.id !== id);
		this.groupIndex = Math.min(this.groupIndex, Math.max(0, this.groups.length - 1));
		this.photoIndex = 0;
	}

	#replace(id: string, group: DuplicateGroup): void {
		const shownId = this.shown?.asset.id;
		this.groups = this.groups.map((g) => (g.id === id ? group : g));
		const index = this.photos.findIndex((photo) => photo.asset.id === shownId);
		this.photoIndex = Math.max(0, index);
	}
}
