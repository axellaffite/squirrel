/**
 * Downloading selected photos: one photo is its original file, several are one zip the server
 * prepares (`POST /api/files/archive`) and the browser then saves natively from a link.
 */
import { api, errorMessage } from '#lib/api/client.ts';
import type { components } from '#lib/api/schema.d.ts';
import type { Outcome } from '#lib/files/pages.ts';
import { contentUrl } from '#lib/files/format.ts';

export type ArchiveTicket = components['schemas']['ArchiveTicket'];
export type ArchiveRequest = components['schemas']['CreateArchiveRequest'];
export type ArchiveCall = (body: ArchiveRequest) => Promise<Outcome<ArchiveTicket>>;

/** Files the server puts in one zip at most. */
export const MAX_ARCHIVE = 500;

const TOO_MANY = `Select at most ${MAX_ARCHIVE} photos to download at once.`;
const TOO_LARGE = 'Too much to put in one zip: select fewer photos.';
const GONE = 'Some of these photos are no longer available. Reload the page and try again.';

export type DownloadPlan =
	| { kind: 'none' }
	| { kind: 'direct'; url: string }
	| { kind: 'archive'; body: ArchiveRequest }
	| { kind: 'too-many'; message: string };

/** What downloading `ids` (in the order given) takes. */
export function planDownload(ids: readonly string[]): DownloadPlan {
	if (ids.length === 0) return { kind: 'none' };
	if (ids.length === 1) return { kind: 'direct', url: contentUrl(ids[0], true) };
	if (ids.length > MAX_ARCHIVE) return { kind: 'too-many', message: TOO_MANY };
	return { kind: 'archive', body: { node_ids: [...ids] } };
}

export function archiveUrl(ticket: string): string {
	return `/api/files/archive/${encodeURIComponent(ticket)}`;
}

/**
 * The Download button's accessible name: the photo's name when one is selected and loaded
 * (`known` holds the selected photos found on the page), else the count.
 */
export function downloadLabel(known: readonly { id: string; name: string }[], count: number): string {
	if (count === 1 && known.length === 1) return `Download ${known[0].name}`;
	return `Download ${count} ${count === 1 ? 'photo' : 'photos'}`;
}

/** The message for a refused zip, worded for the photo pages. */
export function archiveError(error: unknown, status: number): string {
	if (status === 413) return TOO_LARGE;
	if (status === 404) return GONE;
	return errorMessage(error, new Response(null, { status: status >= 200 && status <= 599 ? status : 500 }));
}

export const createArchive: ArchiveCall = async (body) => {
	try {
		const { data, error, response } = await api.POST('/api/files/archive', { body });
		if (response.ok && data) return { ok: true, data };
		return { ok: false, error: archiveError(error, response.status), status: response.status };
	} catch {
		return { ok: false, error: archiveError(null, 0), status: 0 };
	}
};

/** Saves `url` to disk through a temporary link: the browser streams it, nothing is held in memory. */
export function startDownload(url: string): void {
	const link = document.createElement('a');
	link.href = url;
	link.download = '';
	link.rel = 'noopener';
	link.hidden = true;
	document.body.append(link);
	link.click();
	link.remove();
}

interface DownloadDeps {
	createArchive: ArchiveCall;
	start: (url: string) => void;
}

/** Downloads `ids`; resolves to an error message, or null once the download started. */
export async function downloadPhotos(
	ids: readonly string[],
	deps: DownloadDeps = { createArchive, start: startDownload }
): Promise<string | null> {
	const plan = planDownload(ids);
	switch (plan.kind) {
		case 'none':
			return null;
		case 'too-many':
			return plan.message;
		case 'direct':
			deps.start(plan.url);
			return null;
		case 'archive': {
			const result = await deps.createArchive(plan.body);
			if (!result.ok) return result.error;
			deps.start(archiveUrl(result.data.ticket));
			return null;
		}
	}
}
