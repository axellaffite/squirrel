import { api, onStaleCsrf, onUnauthorized, setCsrfToken } from '#lib/api/client.ts';
import { interpretSession, logoutOutcome, type LogoutOutcome, type Me } from '#lib/session-logic.ts';

export type { Me } from '#lib/session-logic.ts';

interface SessionState {
	me: Me | null;
	needsSetup: boolean;
	loaded: boolean;
	unreachable: boolean;
}

export const session: SessionState = $state({
	me: null,
	needsSetup: false,
	loaded: false,
	unreachable: false
});

export function setMe(me: Me | null): void {
	session.me = me;
	setCsrfToken(me?.csrf_token ?? null);
}

let pendingLoad: Promise<void> | null = null;

/** Loads setup status and the current user; concurrent calls share one request pair. */
export function loadSession(): Promise<void> {
	pendingLoad ??= fetchSession().finally(() => {
		pendingLoad = null;
	});
	return pendingLoad;
}

async function fetchSession(): Promise<void> {
	try {
		const [status, me] = await Promise.all([
			api.GET('/api/setup/status'),
			api.GET('/api/auth/me')
		]);
		const outcome = interpretSession(status, me);
		session.unreachable = outcome.unreachable;
		if (!outcome.unreachable) {
			session.needsSetup = outcome.needsSetup;
			setMe(outcome.me);
		}
	} catch {
		session.unreachable = true;
	} finally {
		session.loaded = true;
	}
}

/** Logs out. On failure the session is kept, so the user can retry. */
export async function logout(): Promise<LogoutOutcome> {
	let status: number | null = null;
	try {
		const { response } = await api.POST('/api/auth/logout');
		status = response.status;
	} catch {
		// Network failure: keep the session, report the error below.
	}
	const outcome = logoutOutcome(status);
	if (outcome.loggedOut) setMe(null);
	return outcome;
}

onUnauthorized(() => setMe(null));
onStaleCsrf(loadSession);
