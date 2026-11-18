import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { expect, test, type Page } from '@playwright/test';
import type { UploadFile } from './files.ts';

/** Inference, clustering and tagging are awaited at most this long. */
export const ML_TIMEOUT = 180_000;

const PORTRAITS = fileURLToPath(new URL('../../../crates/squirrel-ml/tests/fixtures/faces/', import.meta.url));
const MIRRORED = fileURLToPath(new URL('../fixtures/faces/', import.meta.url));

interface MlStatus {
	ml: { queued: number; state: string; models_installed: { clip: boolean; faces: boolean } };
	queued: number;
	processing: number;
}

async function status(page: Page): Promise<MlStatus> {
	return (await (await page.request.get('/api/media/status')).json()) as MlStatus;
}

/**
 * Skips an ML scenario on a server without the models (a developer machine without
 * scripts/fetch-models.sh); on CI, where they are installed, a missing model fails instead.
 */
export async function requireModels(page: Page, group: 'clip' | 'faces'): Promise<void> {
	const installed = (await status(page)).ml.models_installed[group];
	if (process.env.CI) expect(installed, `${group} models on the CI server`).toBe(true);
	test.skip(!installed, `the E2E server has no ${group} models (scripts/fetch-models.sh)`);
}

/** Waits until every photo is prepared and analysed. */
export async function waitForMl(page: Page): Promise<void> {
	await expect
		.poll(
			async () => {
				const s = await status(page);
				return s.queued + s.processing + s.ml.queued;
			},
			{ timeout: ML_TIMEOUT, intervals: [1000, 2000, 3000] }
		)
		.toBe(0);
}

export interface PersonSummary {
	id: string;
	name: string | null;
	face_count: number;
	hidden: boolean;
}

export async function people(page: Page, includeHidden = false): Promise<PersonSummary[]> {
	const response = await page.request.get(`/api/people?include_hidden=${includeHidden}`);
	expect(response.status()).toBe(200);
	return (await response.json()) as PersonSummary[];
}

/** Waits until clustering has made `count` people. */
export async function waitForPeople(page: Page, count: number): Promise<PersonSummary[]> {
	await expect.poll(async () => (await people(page)).length, { timeout: ML_TIMEOUT, intervals: [1000, 2000, 3000] }).toBe(count);
	return people(page);
}

/** The four portraits and their mirror images: four photos of each of two people. */
export function portraits(): UploadFile[] {
	return ['koch-a', 'koch-b', 'glover-a', 'glover-b'].flatMap((name) => [
		{ name: `${name}.jpg`, mimeType: 'image/jpeg', buffer: readFileSync(`${PORTRAITS}${name}.jpg`) },
		{ name: `${name}-mirrored.jpg`, mimeType: 'image/jpeg', buffer: readFileSync(`${MIRRORED}${name}-mirrored.jpg`) }
	]);
}

/** One original portrait (the same bytes as in `portraits()`). */
export function portrait(name: string): UploadFile {
	return { name: `${name}.jpg`, mimeType: 'image/jpeg', buffer: readFileSync(`${PORTRAITS}${name}.jpg`) };
}
