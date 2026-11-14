/** The fields the home page reads from a node (so tests can build plain objects). */
interface HomeNode {
	id: string;
	name: string;
	modified_at: string;
}

/** The most recently changed items, newest first, at most `count`. */
export function recentItems<T extends HomeNode>(items: readonly T[], count: number): T[] {
	return [...items].sort((a, b) => Date.parse(b.modified_at) - Date.parse(a.modified_at)).slice(0, count);
}

/** Where the home upload button puts files: the "Files" folder, else the first top-level folder. */
export function uploadTarget<T extends HomeNode>(roots: readonly T[]): T | null {
	return roots.find((root) => root.name === 'Files') ?? roots[0] ?? null;
}
