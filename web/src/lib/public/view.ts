/** What the public link page shows (no I/O). */
import type { AlbumPhoto, LinkInfo, PublicNode } from '#lib/public/api.ts';

/** A file opened in the viewer. */
export interface ViewedFile {
	id: string;
	name: string;
	mime: string | null;
	/** A ready thumbnail, so a preview can be asked for. */
	hasThumbnail: boolean;
}

export const fileOfNode = (node: PublicNode): ViewedFile => ({
	id: node.id,
	name: node.name,
	mime: node.mime ?? null,
	hasThumbnail: node.thumbnail != null
});

export const fileOfPhoto = (photo: AlbumPhoto): ViewedFile => ({
	id: photo.id,
	name: photo.name,
	mime: photo.mime ?? null,
	hasThumbnail: photo.thumbnail.state === 'ready'
});

/** The page title: the folder, file or album name. */
export function linkName(info: LinkInfo): string {
	return info.node?.name ?? info.album?.name ?? 'Shared with you';
}

const dayFormat = new Intl.DateTimeFormat(undefined, { dateStyle: 'medium' });

/** "Shared by Alice · Until 4 Dec 2026" */
export function linkByline(info: LinkInfo): string {
	const parts = [info.shared_by ? `Shared by ${info.shared_by}` : null];
	if (info.expires_at) parts.push(`Until ${dayFormat.format(new Date(info.expires_at))}`);
	return parts.filter((part): part is string => part !== null).join(' · ');
}

/** Files of a listing that a zip can hold (folders cannot go in one). */
export const zippable = (nodes: readonly PublicNode[]): string[] =>
	nodes.filter((node) => node.kind === 'file').map((node) => node.id);

/** The most files one zip holds (the server's limit). */
export const MAX_ZIP_FILES = 500;
