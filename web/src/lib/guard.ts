export interface GuardState {
	loaded: boolean;
	unreachable: boolean;
	needsSetup: boolean;
	me: unknown;
	path: string;
}

/** Pages an anonymous visitor may see. */
export function isPublicPath(path: string): boolean {
	return path === '/login' || path === '/setup' || path.startsWith('/join/');
}

/**
 * Where the current visitor must be sent, or null when the page at `path` may render.
 * Returns null while the session is loading or the server is unreachable: the layout
 * shows its own screens for those states.
 */
export function redirectFor({ loaded, unreachable, needsSetup, me, path }: GuardState): string | null {
	if (!loaded || unreachable) return null;
	if (needsSetup) return path === '/setup' ? null : '/setup';
	if (path === '/setup') return me ? '/' : '/login';
	if (!me) return isPublicPath(path) ? null : '/login';
	if (path === '/login') return '/';
	return null;
}
