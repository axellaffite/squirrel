/**
 * Adding photos to an album in request-sized batches, safe to retry: an album created for them
 * is kept (never created twice) and batches already added are not sent again.
 */
import { addedMessage, batches } from '#lib/photos/albums.ts';
import { addToAlbum, createAlbum, type Album, type Outcome } from '#lib/photos/api.ts';

export interface AlbumWrites {
	createAlbum: (name: string, nodeIds: readonly string[]) => Promise<Outcome<Album>>;
	addToAlbum: (id: string, nodeIds: readonly string[]) => Promise<Outcome<{ changed: number }>>;
}

export type AddResult = { ok: true; message: string } | { ok: false; error: string };

export class AlbumAdder {
	readonly #batches: string[][];
	readonly #requested: number;
	readonly #writes: AlbumWrites;
	/** The album being filled and how far it got, kept across retries. */
	#target: { id: string; name: string } | null = null;
	#sent = 0;
	#changed = 0;

	constructor(ids: readonly string[], writes: AlbumWrites = { createAlbum, addToAlbum }) {
		this.#batches = batches(ids);
		this.#requested = ids.length;
		this.#writes = writes;
	}

	/** The album created by `intoNew`, if any (a retry fills it instead of creating another). */
	get created(): { id: string; name: string } | null {
		return this.#target;
	}

	/** Into an existing album. */
	into(album: { id: string; name: string }): Promise<AddResult> {
		if (this.#target?.id !== album.id) this.#start(album);
		return this.#rest();
	}

	/** Into a new album named `name`, created with the first batch. */
	async intoNew(name: string): Promise<AddResult> {
		if (!this.#target) {
			const created = await this.#writes.createAlbum(name, this.#batches[0] ?? []);
			if (!created.ok) return { ok: false, error: created.error };
			this.#start(created.data);
			this.#sent = 1;
			this.#changed = created.data.item_count;
		}
		return this.#rest();
	}

	#start(album: { id: string; name: string }) {
		this.#target = { id: album.id, name: album.name };
		this.#sent = 0;
		this.#changed = 0;
	}

	async #rest(): Promise<AddResult> {
		const target = this.#target;
		if (!target) return { ok: false, error: 'No album chosen.' };
		while (this.#sent < this.#batches.length) {
			const result = await this.#writes.addToAlbum(target.id, this.#batches[this.#sent]);
			if (!result.ok) return { ok: false, error: result.error };
			this.#changed += result.data.changed;
			this.#sent += 1;
		}
		return { ok: true, message: addedMessage(this.#changed, this.#requested, target.name) };
	}
}
