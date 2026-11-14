import type { IconName } from '#lib/components/icons/paths.ts';
import { previewKind } from '#lib/files/format.ts';

const BY_PREVIEW: Record<string, IconName> = {
	image: 'image',
	video: 'film',
	audio: 'music',
	pdf: 'file-text',
	text: 'file-text'
};

/** The icon for a file list row: folders, then by what the file holds. */
export function nodeIcon(node: { kind: string; mime?: string | null }): IconName {
	if (node.kind === 'folder') return 'folder';
	const kind = previewKind(node.mime);
	return kind ? BY_PREVIEW[kind] : 'file';
}
