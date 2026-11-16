import type { components } from '#lib/api/schema.d.ts';

export type Device = components['schemas']['DeviceDto'];

const PLATFORMS: Record<Device['platform'], string> = { android: 'Android', ios: 'iOS' };
const MINUTE_MS = 60 * 1000;
const HOUR_MS = 60 * MINUTE_MS;
const DAY_MS = 24 * HOUR_MS;
/** Seen this recently counts as in use right now. */
const ACTIVE_MS = 5 * MINUTE_MS;

/** "Pixel 8 · Android · 0.1.0" (the model is left out when unknown). */
export function deviceLabel(device: Pick<Device, 'model' | 'platform' | 'app_version'>): string {
	return [device.model, PLATFORMS[device.platform], device.app_version].filter(Boolean).join(' · ');
}

/** When the device was last used: "Active now", "12 min ago", "2 h ago", then "14 Nov". */
export function lastSeen(device: Pick<Device, 'last_seen_at'>, now: Date): string {
	const seen = new Date(device.last_seen_at);
	const ago = now.getTime() - seen.getTime();
	if (ago < ACTIVE_MS) return 'Active now';
	if (ago < HOUR_MS) return `${Math.floor(ago / MINUTE_MS)} min ago`;
	if (ago < DAY_MS) return `${Math.floor(ago / HOUR_MS)} h ago`;
	const sameYear = seen.getFullYear() === now.getFullYear();
	return new Intl.DateTimeFormat('en-GB', {
		day: 'numeric',
		month: 'short',
		...(sameYear ? {} : { year: 'numeric' })
	}).format(seen);
}

/** Live devices first, most recently seen first; then revoked ones, most recently revoked first. */
export function sortDevices<T extends Pick<Device, 'last_seen_at' | 'revoked_at'>>(devices: readonly T[]): T[] {
	const time = (value: string | null | undefined) => (value ? Date.parse(value) : 0);
	return [...devices].sort((a, b) => {
		const revoked = Number(Boolean(a.revoked_at)) - Number(Boolean(b.revoked_at));
		if (revoked !== 0) return revoked;
		return a.revoked_at
			? time(b.revoked_at) - time(a.revoked_at)
			: time(b.last_seen_at) - time(a.last_seen_at);
	});
}
