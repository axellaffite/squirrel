import { describe, expect, it } from 'vitest';
import { progressPercent, statusLabel, uploadSummary, type QueueEntry } from '#lib/upload/queue-logic.ts';

const entry = (over: Partial<QueueEntry>): QueueEntry => ({
	status: 'queued',
	done: 0,
	total: 100,
	error: null,
	deduplicated: false,
	finalName: null,
	name: 'a.bin',
	...over
});

describe('statusLabel', () => {
	it.each([
		[entry({}), 'Waiting'],
		[entry({ status: 'hashing', done: 40 }), 'Checking 40%'],
		[entry({ status: 'uploading', done: 1500, total: 3000 }), 'Uploading 1.5 KB of 3.0 KB'],
		[entry({ status: 'finishing' }), 'Finishing'],
		[entry({ status: 'done', deduplicated: true }), 'Already on your drive'],
		[entry({ status: 'done', deduplicated: true, finalName: 'a (1).bin' }), 'Already on your drive, saved as a (1).bin'],
		[entry({ status: 'done', finalName: 'a (1).bin' }), 'Saved as a (1).bin'],
		[entry({ status: 'done', finalName: 'a.bin' }), 'Done'],
		[entry({ status: 'failed', error: 'this would exceed your storage quota' }), 'this would exceed your storage quota'],
		[entry({ status: 'cancelled' }), 'Cancelled']
	])('%o → %s', (item, expected) => expect(statusLabel(item)).toBe(expected));
});

describe('progressPercent', () => {
	it('handles empty files and clamps', () => {
		expect(progressPercent(entry({ total: 0 }))).toBe(0);
		expect(progressPercent(entry({ status: 'done', total: 0 }))).toBe(100);
		expect(progressPercent(entry({ done: 50 }))).toBe(50);
	});
});

describe('uploadSummary', () => {
	it('is empty without uploads', () => {
		expect(uploadSummary([])).toBe('');
	});

	it('counts running uploads, ignoring byte progress', () => {
		const items = [entry({ status: 'uploading', done: 10 }), entry({ status: 'queued' }), entry({ status: 'done' })];
		expect(uploadSummary(items)).toBe('Uploading 2 files, 1 done.');
		items[0] = entry({ status: 'uploading', done: 90 });
		expect(uploadSummary(items)).toBe('Uploading 2 files, 1 done.');
	});

	it('names failures once everything settled', () => {
		expect(uploadSummary([entry({ status: 'done' }), entry({ status: 'failed', name: 'b.bin' })])).toBe(
			'Uploads finished. 1 failed: b.bin.'
		);
		expect(uploadSummary([entry({ status: 'hashing' })])).toBe('Uploading 1 file.');
		expect(uploadSummary([entry({ status: 'done' }), entry({ status: 'cancelled' })])).toBe('Uploads finished.');
	});
});
