/** One album's details and photos, paged by cursor; answers from before a reload are dropped. */
import { batches, photoCount } from '#lib/photos/albums.ts';
import {
	getAlbum,
	listAlbumPage,
	removeFromAlbum,
	type Album,
	type AssetSummary,
	type Outcome,
	type TimelinePage
} from '#lib/photos/api.ts';

export interface AlbumSource {
	getAlbum: (id: string) => Promise<Outcome<Album>>;
	listAlbumPage: (id: string, cursor: string | null) => Promise<Outcome<TimelinePage>>;
	removeFromAlbum: (id: string, nodeIds: readonly string[]) => Promise<Outcome<{ changed: number }>>;
}

export class AlbumData {
	album: Album | null = $state(null);
	items: AssetSummary[] = $state.raw([]);
	finished = $state(false);
	/** About the album itself (e.g. it no longer exists). Never overwritten by page errors. */
	albumError: string | null = $state(null);
	/** About loading its photos. */
	pageError: string | null = $state(null);

	readonly #id: string;
	readonly #source: AlbumSource;
	#cursor: string | null = null;
	#loadingPage = false;
	#generation = 0;

	constructor(id: string, source: AlbumSource = { getAlbum, listAlbumPage, removeFromAlbum }) {
		this.#id = id;
		this.#source = source;
	}

	async loadAlbum(): Promise<void> {
		const generation = this.#generation;
		const result = await this.#source.getAlbum(this.#id);
		if (generation !== this.#generation) return;
		if (result.ok) {
			this.album = result.data;
			this.albumError = null;
		} else {
			this.albumError = result.status === 404 ? 'This album does not exist or was deleted.' : result.error;
		}
	}

	/** The next page of photos; ignored while one is loading or when all are in. */
	async nextPage(): Promise<void> {
		if (this.#loadingPage || this.finished) return;
		const generation = this.#generation;
		this.#loadingPage = true;
		const result = await this.#source.listAlbumPage(this.#id, this.#cursor);
		if (generation !== this.#generation) return;
		this.#loadingPage = false;
		if (!result.ok) {
			this.pageError = result.error;
			this.finished = true;
			return;
		}
		this.pageError = null;
		this.items = [...this.items, ...result.data.items];
		this.#cursor = result.data.next_cursor ?? null;
		this.finished = this.#cursor === null;
	}

	/** After a change: the album (count, cover) and its photos from the first page. */
	async reload(): Promise<void> {
		this.#generation += 1;
		this.#loadingPage = false;
		this.#cursor = null;
		this.finished = false;
		this.items = [];
		await Promise.all([this.loadAlbum(), this.nextPage()]);
	}

	/** Takes photos out of the album, a batch at a time; reports what was removed and any error. */
	async remove(ids: readonly string[]): Promise<{ removed: number; message: string | null; error: string | null }> {
		let removed = 0;
		let error: string | null = null;
		for (const batch of batches(ids)) {
			const result = await this.#source.removeFromAlbum(this.#id, batch);
			if (!result.ok) {
				error = result.error;
				break;
			}
			removed += result.data.changed;
		}
		await this.reload();
		const message = removed > 0 ? `Removed ${photoCount(removed)} from the album.` : null;
		return { removed, message, error };
	}

	/** Shows a deleted photo's absence at once, before the reload confirms it. */
	drop(id: string): void {
		this.items = this.items.filter((item) => item.id !== id);
	}
}
