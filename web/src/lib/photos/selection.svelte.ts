/** Multi-select on the timeline: ids, so the selection survives month loading and relayout. */
export class Selection {
	ids: ReadonlySet<string> = $state.raw(new Set());
	/** The last tile toggled: one end of a shift-click range. */
	#anchor: string | null = null;

	get size(): number {
		return this.ids.size;
	}

	/** Selection mode is on while anything is selected. */
	get active(): boolean {
		return this.ids.size > 0;
	}

	has(id: string): boolean {
		return this.ids.has(id);
	}

	toggle(id: string): void {
		const next = new Set(this.ids);
		if (next.has(id)) next.delete(id);
		else next.add(id);
		this.ids = next;
		this.#anchor = id;
	}

	/**
	 * Selects everything between the anchor and `id` in `order` (the loaded photos in timeline
	 * order). Without an anchor in `order`, it toggles `id` alone.
	 */
	selectRange(id: string, order: readonly string[]): void {
		const from = this.#anchor === null ? -1 : order.indexOf(this.#anchor);
		const to = order.indexOf(id);
		if (from < 0 || to < 0) {
			this.toggle(id);
			return;
		}
		const [start, end] = from < to ? [from, to] : [to, from];
		this.ids = new Set([...this.ids, ...order.slice(start, end + 1)]);
		this.#anchor = id;
	}

	/** Drops ids that are gone (e.g. deleted). */
	keepOnly(present: ReadonlySet<string>): void {
		const next = new Set([...this.ids].filter((id) => present.has(id)));
		if (next.size !== this.ids.size) this.ids = next;
	}

	clear(): void {
		this.ids = new Set();
		this.#anchor = null;
	}
}
