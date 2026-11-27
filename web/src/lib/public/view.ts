/** What the public link page shows (no I/O). */
import type { IconName } from '#lib/components/icons/paths.ts';
import { formatBytes } from '#lib/files/format.ts';
import { nodeIcon } from '#lib/files/nodeIcon.ts';
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

/** "Shared by Alice · 2.2 MB" for a file, "· 12 photos" for an album, nothing more for a folder. */
export function linkDetails(info: LinkInfo): string {
	const parts = [info.shared_by ? `Shared by ${info.shared_by}` : null];
	if (info.node?.kind === 'file') parts.push(formatBytes(info.node.size));
	if (info.album) parts.push(`${info.album.item_count} ${info.album.item_count === 1 ? 'photo' : 'photos'}`);
	return parts.filter((part): part is string => part !== null).join(' · ');
}

/** "This link expires on 4 Dec 2026", or `null` when it never does. */
export function linkExpiry(info: LinkInfo): string | null {
	return info.expires_at ? `This link expires on ${dayFormat.format(new Date(info.expires_at))}` : null;
}

/** The icon beside the link's name. */
export function linkIcon(info: LinkInfo): IconName {
	if (info.album) return 'images';
	return info.node ? nodeIcon(info.node) : 'link';
}

/** Files of a listing that a zip can hold (folders cannot go in one). */
export const zippable = (nodes: readonly PublicNode[]): string[] =>
	nodes.filter((node) => node.kind === 'file').map((node) => node.id);

/** The most files one zip holds (the server's limit). */
export const MAX_ZIP_FILES = 500;
