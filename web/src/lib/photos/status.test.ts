import { describe, expect, it, vi } from 'vitest';
import { createPoller, preparingText } from '#lib/photos/status.ts';

const ml = {
	state: 'unavailable' as const,
	reason: null,
	queued: 0,
	failed: 0,
	models_installed: { clip: false, faces: false },
};
const status = (queued: number, processing = 0) => ({ ok: true as const, data: { queued, processing, failed: 0, ml } });
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

function harness(answers: ReturnType<typeof status>[]) {
	const timers: (() => void)[] = [];
	const onCount = vi.fn();
	const onSettled = vi.fn();
	const poller = createPoller({
		fetch: vi.fn(async () => answers.shift() ?? status(0)),
		onCount,
		onSettled,
		schedule: (run) => {
			timers.push(run);
			return () => timers.splice(timers.indexOf(run), 1);
		}
	});
	const advance = async () => {
		timers.shift()?.();
		await flush();
	};
	return { poller, timers, onCount, onSettled, advance };
}

describe('createPoller', () => {
	it('polls while files are pending and reports when they are done', async () => {
		const h = harness([status(3), status(1, 1), status(0)]);
		h.poller.start();
		await flush();
		expect(h.onCount).toHaveBeenLastCalledWith(3);
		await h.advance();
		expect(h.onCount).toHaveBeenLastCalledWith(2);
		await h.advance();
		expect(h.onCount).toHaveBeenLastCalledWith(0);
		expect(h.onSettled).toHaveBeenCalledOnce();
		expect(h.timers).toHaveLength(0);
	});

	it('asks once and stops when nothing is pending', async () => {
		const h = harness([status(0)]);
		h.poller.start();
		await flush();
		expect(h.onSettled).not.toHaveBeenCalled();
		expect(h.timers).toHaveLength(0);
	});

	it('stops for good', async () => {
		const h = harness([status(2)]);
		h.poller.start();
		await flush();
		h.poller.stop();
		expect(h.timers).toHaveLength(0);
		h.poller.start();
		await flush();
		expect(h.onCount).toHaveBeenCalledTimes(1);
	});
});

describe('createPoller after uploads', () => {
	it('refreshes once even when processing finished before the first poll', async () => {
		const h = harness([status(0)]);
		h.poller.start({ assumePending: true });
		await flush();
		expect(h.onSettled).toHaveBeenCalledOnce();
		expect(h.timers).toHaveLength(0);
	});

	it('keeps a single timer when started again while waiting', async () => {
		const h = harness([status(2), status(2), status(0)]);
		h.poller.start();
		await flush();
		h.poller.start();
		await flush();
		expect(h.timers).toHaveLength(1);
	});

	it('pauses while the page is hidden', async () => {
		let shown = false;
		const fetch = vi.fn(async () => status(0));
		const timers: (() => void)[] = [];
		const poller = createPoller({
			fetch,
			onCount: () => {},
			onSettled: () => {},
			visible: () => shown,
			schedule: (run) => {
				timers.push(run);
				return () => {};
			}
		});
		poller.start();
		await flush();
		expect(fetch).not.toHaveBeenCalled();
		shown = true;
		timers.shift()?.();
		await flush();
		expect(fetch).toHaveBeenCalledOnce();
	});
});

describe('createPoller progress', () => {
	it('refreshes when some files are done, at most every 15 s', async () => {
		let clock = 0;
		const timers: (() => void)[] = [];
		const answers = [status(5), status(4), status(3), status(2)];
		const onProgress = vi.fn();
		const poller = createPoller({
			fetch: async () => answers.shift() ?? status(2),
			onCount: () => {},
			onSettled: () => {},
			onProgress,
			now: () => clock,
			schedule: (run) => {
				timers.push(run);
				return () => {};
			}
		});
		poller.start();
		await flush();
		clock = 5000;
		timers.shift()?.();
		await flush();
		expect(onProgress).toHaveBeenCalledTimes(1);
		clock = 10_000;
		timers.shift()?.();
		await flush();
		expect(onProgress).toHaveBeenCalledTimes(1);
		clock = 20_000;
		timers.shift()?.();
		await flush();
		expect(onProgress).toHaveBeenCalledTimes(2);
	});
});

describe('preparingText', () => {
	it('counts files', () => {
		expect(preparingText(1)).toBe('Preparing 1 photo or document…');
		expect(preparingText(12)).toBe('Preparing 12 photos and documents…');
	});
});
