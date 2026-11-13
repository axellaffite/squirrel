import { api, setCsrfToken } from './api/client';
import type { components } from './api/schema';

export type Me = components['schemas']['MeResponse'];

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

export async function loadSession(): Promise<void> {
	try {
		const [status, me] = await Promise.all([
			api.GET('/api/setup/status'),
			api.GET('/api/auth/me')
		]);
		session.unreachable = !status.data;
		session.needsSetup = status.data?.needs_setup ?? false;
		setMe(me.data ?? null);
	} catch {
		session.unreachable = true;
	} finally {
		session.loaded = true;
	}
}

export async function logout(): Promise<void> {
	await api.POST('/api/auth/logout');
	setMe(null);
}
