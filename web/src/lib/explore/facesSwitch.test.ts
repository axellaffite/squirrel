import { describe, expect, it, vi } from 'vitest';
import { FacesSwitch, OFF_NOTICE, countsText, type FacesApi } from '#lib/explore/facesSwitch.svelte.ts';

const settings = (enabled: boolean, people = 2, faces = 8) => ({ faces_enabled: enabled, people_count: people, faces_count: faces });

function api(answers: Partial<FacesApi> = {}): FacesApi {
	return {
		get: answers.get ?? vi.fn(async () => ({ ok: true as const, data: settings(true) })),
		set: answers.set ?? vi.fn(async (enabled: boolean) => ({ ok: true as const, data: settings(enabled, enabled ? 2 : 0, enabled ? 8 : 0) }))
	};
}

describe('FacesSwitch', () => {
	it('asks before turning off, with counts fetched when it asks', async () => {
		let people = 2;
		const get = vi.fn(async () => ({ ok: true as const, data: settings(true, people, 8) }));
		const sw = new FacesSwitch(api({ get }));
		await sw.load();
		people = 5; // more people were found since the page loaded
		const asking = sw.toggle(false);
		expect(sw.checked).toBe(true);
		expect(sw.confirming).toBe(true);
		expect(sw.confirmCounts).toBeNull();
		await asking;
		expect(sw.confirmCounts).toEqual({ people: 5, faces: 8 });
		expect(countsText(sw.confirmCounts!)).toBe('5 people and 8 faces');
	});

	it('turns off once confirmed, keeping the dialog text while it closes', async () => {
		const sw = new FacesSwitch(api());
		await sw.load();
		await sw.toggle(false);
		expect(await sw.turnOff()).toBeNull();
		expect(sw.checked).toBe(false);
		expect(sw.notice).toBe(OFF_NOTICE);
		expect(sw.confirmCounts).toEqual({ people: 2, faces: 8 });
		expect(sw.settings?.people_count).toBe(0);
		sw.closed();
		expect(sw.confirming).toBe(false);
	});

	it('stays on when the confirmation is cancelled', async () => {
		const set = vi.fn(async () => ({ ok: true as const, data: settings(false, 0, 0) }));
		const sw = new FacesSwitch(api({ set }));
		await sw.load();
		await sw.toggle(false);
		sw.closed();
		expect(sw.checked).toBe(true);
		expect(set).not.toHaveBeenCalled();
		// Asking again is a new question.
		const asks = sw.asks;
		await sw.toggle(false);
		expect(sw.confirming).toBe(true);
		expect(sw.asks).toBe(asks + 1);
	});

	it('reports a failure and keeps the switch where it was', async () => {
		const set = vi.fn(async () => ({ ok: false as const, error: 'The server is busy.', status: 503 }));
		const sw = new FacesSwitch(api({ set }));
		await sw.load();
		await sw.toggle(false);
		expect(await sw.turnOff()).toBe('The server is busy.');
		expect(sw.checked).toBe(true);
		const off = new FacesSwitch(api({ get: async () => ({ ok: true, data: settings(false) }), set }));
		await off.load();
		await off.toggle(true);
		expect(off.checked).toBe(false);
		expect(off.error).toBe('The server is busy.');
	});

	it('clears an old error once turning off works', async () => {
		const sw = new FacesSwitch(api());
		await sw.load();
		sw.error = 'old';
		await sw.toggle(false);
		await sw.turnOff();
		expect(sw.error).toBeNull();
	});

	it('never lets an older read overwrite a newer one', async () => {
		let release: (() => void) | undefined;
		let calls = 0;
		const get = vi.fn(() => {
			calls += 1;
			if (calls === 1) return new Promise<{ ok: true; data: ReturnType<typeof settings> }>((resolve) => (release = () => resolve({ ok: true, data: settings(true, 1, 1) })));
			return Promise.resolve({ ok: true as const, data: settings(true, 3, 9) });
		});
		const sw = new FacesSwitch(api({ get }));
		const first = sw.load();
		await sw.load();
		release?.();
		await first;
		expect(sw.settings?.people_count).toBe(3);
	});
});
