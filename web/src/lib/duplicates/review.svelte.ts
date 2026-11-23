/**
 * The duplicates review: the groups loaded so far (one grid tile each, showing the suggested
 * keeper), the group open in the photo viewer, and the photo of that group shown in it.
 * Requests are injected, so the page and the tests drive the same logic.
 */
import type { DuplicateGroup, DuplicateMember, DuplicatesPage } from '#lib/duplicates/api.ts';
import { reviewOrder, suggestedKeeper } from '#lib/duplicates/model.ts';
import type { Outcome } from '#lib/files/pages.ts';
import type { AssetSummary } from '#lib/photos/api.ts';

export interface ReviewActions {
	keep: (group: string, asset: string) => Promise<Outcome<DuplicateGroup>>;
	restore: (group: string, asset: string | null) => Promise<Outcome<DuplicateGroup | null>>;
}

export class DuplicatesReview {
	groups: DuplicateGroup[] = $state.raw([]);
	/** Where the next page starts; `null` once everything is loaded. */
	cursor: string | null = $state(null);
	/** Open groups on the server (all pages), as last reported. */
	openCount = $state(0);
	/** The group open in the viewer, as an index in `groups` (`-1`: none). */
	groupIndex = $state(-1);
	/** The photo shown, as an index in the group's review order. */
	photoIndex = $state(0);
	busy = $state(false);
	error: string | null = $state(null);

	readonly #actions: ReviewActions;
	/** Open groups leave the list once a photo is kept; resolved ones stay to be restored. */
	readonly #keepsResolved: boolean;

	constructor(actions: ReviewActions, options: { keepsResolved?: boolean } = {}) {
		this.#actions = actions;
		this.#keepsResolved = options.keepsResolved ?? false;
	}

	/** The first page (replaces what was loaded). */
	load(page: DuplicatesPage): void {
		this.groups = [...page.items];
		this.cursor = page.next_cursor ?? null;
		this.openCount = page.open_count;
		this.groupIndex = -1;
		this.photoIndex = 0;
	}

	/** The next page, after what is loaded. */
	append(page: DuplicatesPage): void {
		const known = new Set(this.groups.map((group) => group.id));
		this.groups = [...this.groups, ...page.items.filter((group) => !known.has(group.id))];
		this.cursor = page.next_cursor ?? null;
		this.openCount = page.open_count;
	}

	/** One tile per group: its suggested keeper. */
	get tiles(): AssetSummary[] {
		return this.groups.map((group) => suggestedKeeper(group.members)?.asset).filter((a): a is AssetSummary => !!a);
	}

	/** How many photos the group behind a tile holds (its badge), or null. */
	countFor(tile: string): number | null {
		const group = this.groups.find((g) => suggestedKeeper(g.members)?.asset.id === tile);
		return group ? group.members.length : null;
	}

	get group(): DuplicateGroup | null {
		return this.groups[this.groupIndex] ?? null;
	}

	/** The tile of the group open in the viewer (the viewer's ←/→ step from it). */
	get tile(): AssetSummary | null {
		return this.group ? (suggestedKeeper(this.group.members)?.asset ?? null) : null;
	}

	/** The group's photos in review order: the suggested keeper first. */
	get photos(): DuplicateMember[] {
		return this.group ? reviewOrder(this.group.members) : [];
	}

	/** The photo shown in the viewer. */
	get shown(): DuplicateMember | null {
		return this.photos[this.photoIndex] ?? null;
	}

	/** Opens the group behind a tile, on its suggested keeper (a click, or the viewer's ←/→). */
	open(tile: string): void {
		const index = this.groups.findIndex((g) => suggestedKeeper(g.members)?.asset.id === tile);
		if (index < 0) return;
		this.groupIndex = index;
		this.photoIndex = 0;
	}

	close(): void {
		this.groupIndex = -1;
		this.photoIndex = 0;
	}

	/** Shows the photo `id` of the open group (a click in the panel). */
	select(id: string): void {
		const index = this.photos.findIndex((photo) => photo.asset.id === id);
		if (index >= 0) this.photoIndex = index;
	}

	/** Moves through the open group's photos (↑/↓), staying within it. */
	step(delta: number): void {
		const count = this.photos.length;
		if (count === 0) return;
		this.photoIndex = Math.min(count - 1, Math.max(0, this.photoIndex + delta));
	}

	/**
	 * Keeps the photo shown; the group's others are hidden. The viewer moves on to the next
	 * group (`group` is null when none is left). Returns the resolved group.
	 */
	async keepShown(): Promise<DuplicateGroup | null> {
		const group = this.group;
		const shown = this.shown;
		if (!group || !shown || this.busy) return null;
		const outcome = await this.#run(() => this.#actions.keep(group.id, shown.asset.id));
		if (!outcome) return null;
		if (this.#keepsResolved) this.#replace(group.id, outcome);
		else {
			this.#drop(group.id);
			this.openCount = Math.max(0, this.openCount - 1);
		}
		return outcome;
	}

	/** Shows hidden photos of the open group again: the photo shown only, or all of them. */
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

	/** Puts a group back first in the list (undoing a keep), closed. */
	restoreToQueue(group: DuplicateGroup): void {
		this.groups = [group, ...this.groups.filter((g) => g.id !== group.id)];
		this.openCount += 1;
		this.close();
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

	/** Removes a group; the viewer goes on to the group that took its place, if any. */
	#drop(id: string): void {
		const index = this.groups.findIndex((group) => group.id === id);
		this.groups = this.groups.filter((group) => group.id !== id);
		if (this.groupIndex >= 0 && index >= 0) {
			this.groupIndex = this.groups.length === 0 ? -1 : Math.min(index, this.groups.length - 1);
		}
		this.photoIndex = 0;
	}

	#replace(id: string, group: DuplicateGroup): void {
		const shownId = this.shown?.asset.id;
		this.groups = this.groups.map((g) => (g.id === id ? group : g));
		const index = this.photos.findIndex((photo) => photo.asset.id === shownId);
		this.photoIndex = Math.max(0, index);
	}
}
