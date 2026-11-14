import { describe, expect, it } from 'vitest';
import { latestOnly } from '#lib/files/latest.ts';

function deferred<T>() {
	let resolve!: (value: T) => void;
	const promise = new Promise<T>((res) => (resolve = res));
	return { promise, resolve };
}

describe('latestOnly', () => {
	it('marks every response but the newest request as stale', async () => {
		const track = latestOnly();
		const first = deferred<string>();
		const second = deferred<string>();
		const a = track(first.promise);
		const b = track(second.promise);
		second.resolve('new');
		first.resolve('old');
		expect(await b).toEqual({ stale: false, value: 'new' });
		expect(await a).toEqual({ stale: true });
	});

	it('passes a lone request through', async () => {
		const track = latestOnly();
		expect(await track(Promise.resolve(1))).toEqual({ stale: false, value: 1 });
		expect(await track(Promise.resolve(2))).toEqual({ stale: false, value: 2 });
	});

	it('rethrows only the newest failure', async () => {
		const track = latestOnly();
		const old = track(Promise.reject(new Error('old')));
		const fresh = track(Promise.reject(new Error('fresh')));
		expect(await old).toEqual({ stale: true });
		await expect(fresh).rejects.toThrow('fresh');
	});
});
