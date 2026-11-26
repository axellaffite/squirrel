/**
 * Which photos show blurred: the caller's photos matching their blur filter (the server lists
 * them), unless revealed one by one or all at once for this visit. Until the list first
 * arrives, every photo is blurred if the filter was on at the last visit (this browser
 * remembers), so a hidden photo never flashes on opening the drive.
 */
export interface BlurState {
	/** Node ids the server lists as blurred; null before the first answer. */
	readonly ids: ReadonlySet<string> | null;
	/** Revealed one by one ("Show"), for this visit. */
	readonly revealed: ReadonlySet<string>;
	/** "Show blurred photos": every one revealed, for this visit. */
	readonly revealAll: boolean;
	/** The filter was on at the last visit: blur everything while the list is loading. */
	readonly wasActive: boolean;
}

export const initialBlurState = (wasActive: boolean): BlurState => ({
	ids: null,
	revealed: new Set(),
	revealAll: false,
	wasActive
});

export function isBlurred(state: BlurState, id: string): boolean {
	if (state.revealAll || state.revealed.has(id)) return false;
	return state.ids === null ? state.wasActive : state.ids.has(id);
}

/** Whether the filter blurs anything (the toggle is offered only then). */
export const hasBlurred = (state: BlurState): boolean => (state.ids?.size ?? 0) > 0;

export const withIds = (state: BlurState, ids: readonly string[]): BlurState => ({
	...state,
	ids: new Set(ids),
	wasActive: ids.length > 0
});

export const withRevealed = (state: BlurState, id: string): BlurState => ({
	...state,
	revealed: new Set([...state.revealed, id])
});

/** Shows every blurred photo, or blurs them all again (forgetting the one-by-one reveals). */
export const withRevealAll = (state: BlurState, revealAll: boolean): BlurState => ({
	...state,
	revealAll,
	revealed: new Set()
});
