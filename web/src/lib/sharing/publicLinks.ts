/** Public links: the owner's calls, and the wording and checks of the share dialog's link section. */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';
import type { Subject } from '#lib/sharing/api.ts';

export type PublicLink = components['schemas']['PublicLinkDto'];
export type PublicLinkList = components['schemas']['PublicLinkList'];
export type CreatedPublicLink = components['schemas']['CreatedPublicLink'];

/** The server's password rules for links (the same as account passwords). */
export const LINK_PASSWORD_MIN = 10;
export const LINK_PASSWORD_MAX = 256;

async function call<T>(request: Promise<{ data?: T; error?: unknown; response: Response }>): Promise<Outcome<T>> {
	try {
		const { data, error, response } = await request;
		if (response.ok) return { ok: true, data: data as T };
		return { ok: false, error: errorMessage(error, response), status: response.status };
	} catch {
		return { ok: false, error: errorMessage(null), status: 0 };
	}
}

export const listPublicLinks = (subject: Subject): Promise<Outcome<PublicLinkList>> =>
	call(api.GET('/api/public-links', { params: { query: subject } }));

export const createPublicLink = (
	subject: Subject,
	options: { expiresAt: string | null; password: string | null }
): Promise<Outcome<CreatedPublicLink>> =>
	call(
		api.POST('/api/public-links', {
			body: { ...subject, expires_at: options.expiresAt, password: options.password }
		})
	);

export const revokePublicLink = (id: string): Promise<Outcome<unknown>> =>
	call(api.DELETE('/api/public-links/{id}', { params: { path: { id } } }));

/**
 * The expiry for a date picked as `YYYY-MM-DD`: the end of that day, local time, as ISO.
 * `null` for an empty pick; an error message for a malformed or past day.
 */
export function expiryFromDay(day: string, now: Date = new Date()): { expiresAt: string | null } | { error: string } {
	if (day.trim() === '') return { expiresAt: null };
	const match = /^(\d{4})-(\d{2})-(\d{2})$/.exec(day.trim());
	if (!match) return { error: 'Pick a valid date.' };
	const [year, month, date] = [Number(match[1]), Number(match[2]) - 1, Number(match[3])];
	const end = new Date(year, month, date, 23, 59, 59);
	if (end.getFullYear() !== year || end.getMonth() !== month || end.getDate() !== date) {
		return { error: 'Pick a valid date.' };
	}
	if (end.getTime() <= now.getTime()) return { error: 'Pick a day from today on.' };
	return { expiresAt: end.toISOString() };
}

/** `null` when `password` is fine for a link (empty means none). */
export function linkPasswordProblem(password: string): string | null {
	if (password === '') return null;
	const length = [...password.normalize('NFKC')].length;
	if (length < LINK_PASSWORD_MIN) return `Use at least ${LINK_PASSWORD_MIN} characters, or leave it empty.`;
	if (length > LINK_PASSWORD_MAX) return `Use at most ${LINK_PASSWORD_MAX} characters.`;
	return null;
}

/** Today as `YYYY-MM-DD` (local), the earliest day an expiry can be picked. */
export function today(now: Date = new Date()): string {
	const pad = (value: number) => String(value).padStart(2, '0');
	return `${now.getFullYear()}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

const dayFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });

/** "Made 27 Nov 2026" */
export const linkTitle = (link: PublicLink): string => `Made ${dayFormat.format(new Date(link.created_at))}`;

/** "Expires 4 Dec 2026 · Password", "Expired", "No expiry". */
export function linkMeta(link: PublicLink): string {
	const expiry = link.expired
		? 'Expired'
		: link.expires_at
			? `Expires ${dayFormat.format(new Date(link.expires_at))}`
			: 'No expiry';
	return link.has_password ? `${expiry} · Password` : expiry;
}
