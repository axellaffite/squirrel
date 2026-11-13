const DAY_MS = 24 * 60 * 60 * 1000;

interface InvitationTimes {
	created_at: string;
	expires_at: string;
}

/** Validity of an invitation in whole days (expires_at − created_at), or null if unknown. */
export function validityDays(invitation: InvitationTimes | null | undefined): number | null {
	if (!invitation) return null;
	const span = Date.parse(invitation.expires_at) - Date.parse(invitation.created_at);
	if (!Number.isFinite(span) || span <= 0) return null;
	return Math.max(1, Math.round(span / DAY_MS));
}

/** The most recently created invitation, which reflects the server's current validity. */
export function newestInvitation<T extends InvitationTimes>(invitations: readonly T[]): T | null {
	return invitations.reduce<T | null>(
		(newest, current) =>
			!newest || Date.parse(current.created_at) > Date.parse(newest.created_at) ? current : newest,
		null
	);
}

/** Sentence describing how long invitation links stay valid. */
export function expiryNotice(days: number | null): string {
	if (days === null) return 'Links expire after a limited time.';
	return `Links expire after ${days} ${days === 1 ? 'day' : 'days'}.`;
}
