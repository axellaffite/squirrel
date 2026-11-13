import type { components } from '#lib/api/schema.d.ts';

export type Me = components['schemas']['MeResponse'];
type SetupStatus = components['schemas']['SetupStatus'];

export interface ApiResult<T> {
	data?: T;
	response: Response;
}

export type SessionOutcome =
	| { unreachable: true }
	| { unreachable: false; needsSetup: boolean; me: Me | null };

/**
 * Interprets the two startup calls. A 401 from `/me` just means "logged out";
 * any other failure (5xx, 429, proxy error page…) means the server is not usable.
 */
export function interpretSession(
	status: ApiResult<SetupStatus>,
	me: ApiResult<Me>
): SessionOutcome {
	if (!status.data) return { unreachable: true };
	if (me.data) return { unreachable: false, needsSetup: status.data.needs_setup, me: me.data };
	if (me.response.status === 401) {
		return { unreachable: false, needsSetup: status.data.needs_setup, me: null };
	}
	return { unreachable: true };
}

export type LogoutOutcome = { loggedOut: true } | { loggedOut: false; error: string };

export const LOGOUT_ERROR = 'Could not log out. Check your connection and try again.';

/** A 401 means the session was already gone, which is as good as a logout. */
export function logoutOutcome(status: number | null): LogoutOutcome {
	if (status !== null && ((status >= 200 && status < 300) || status === 401)) {
		return { loggedOut: true };
	}
	return { loggedOut: false, error: LOGOUT_ERROR };
}
