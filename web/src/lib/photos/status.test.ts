import { describe, expect, it, vi } from 'vitest';
import { createPoller, preparingText } from '#lib/photos/status.ts';

const status = (queued: number, processing = 0) => ({ ok: true as const, data: { queued, processing, failed: 0 } });
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

describe('preparingText', () => {
	it('counts files', () => {
		expect(preparingText(1)).toBe('Preparing 1 photo or document…');
		expect(preparingText(12)).toBe('Preparing 12 photos and documents…');
	});
});
