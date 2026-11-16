/** Polling of `GET /api/media/status` while files are being prepared. */
import type { MediaStatus, Outcome } from '#lib/photos/api.ts';

/** How often the status is asked again while something is pending. */
export const STATUS_POLL_MS = 5000;

/** Files still waiting for, or going through, processing. */
export const pendingCount = (status: MediaStatus): number => status.queued + status.processing;

export function preparingText(count: number): string {
	return count === 1 ? 'Preparing 1 photo or document…' : `Preparing ${count} photos and documents…`;
}

export interface PollerOptions {
	fetch: () => Promise<Outcome<MediaStatus>>;
	/** Called with every answer's pending count. */
	onCount: (count: number) => void;
	/** Called when the count drops from something to zero. */
	onSettled: () => void;
	/** Whether the page is visible; polling pauses (without stopping) while it is not. */
	visible?: () => boolean;
	schedule?: (run: () => void, ms: number) => () => void;
}

/**
 * Asks once, then every `STATUS_POLL_MS` while the count is above zero. `start()` (re)starts it,
 * e.g. after uploads; the returned function stops it for good.
 */
export interface StartOptions {
	/**
	 * Treat files as pending already (e.g. uploads just finished): the first answer of zero then
	 * counts as "done" and calls `onSettled`, even if processing was over before the first poll.
	 */
	assumePending?: boolean;
}

export function createPoller(options: PollerOptions): { start: (opts?: StartOptions) => void; stop: () => void } {
	const schedule =
		options.schedule ??
		((run: () => void, ms: number) => {
			const handle = setTimeout(run, ms);
			return () => clearTimeout(handle);
		});
	const visible = options.visible ?? (() => true);
	let cancel: (() => void) | null = null;
	let last = 0;
	let stopped = false;
	let running = false;

	async function tick() {
		cancel = null;
		if (stopped) return;
		if (!visible()) {
			cancel = schedule(tick, STATUS_POLL_MS);
			return;
		}
		running = true;
		const result = await options.fetch();
		running = false;
		if (stopped) return;
		const count = result.ok ? pendingCount(result.data) : last;
		options.onCount(count);
		if (last > 0 && count === 0) options.onSettled();
		last = count;
		if (count > 0 || !result.ok) cancel = schedule(tick, STATUS_POLL_MS);
	}

	return {
		start(opts: StartOptions = {}) {
			if (stopped) return;
			if (opts.assumePending) last = Math.max(last, 1);
			if (running) return;
			cancel?.();
			void tick();
		},
		stop() {
			stopped = true;
			cancel?.();
		}
	};
}
