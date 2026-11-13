import { formatBytes } from '#lib/files/format.ts';
import type { UploadPhase } from '#lib/upload/uploader.ts';

export type UploadStatus = 'queued' | UploadPhase | 'done' | 'failed' | 'cancelled';

export interface QueueEntry {
	name: string;
	status: UploadStatus;
	done: number;
	total: number;
	error: string | null;
	deduplicated: boolean;
	finalName: string | null;
}

export function progressPercent(entry: QueueEntry): number {
	if (entry.status === 'done') return 100;
	if (entry.total <= 0) return 0;
	return Math.min(100, Math.round((entry.done / entry.total) * 100));
}

export function statusLabel(entry: QueueEntry): string {
	switch (entry.status) {
		case 'queued':
			return 'Waiting';
		case 'hashing':
			return `Checking ${progressPercent(entry)}%`;
		case 'uploading':
			return `Uploading ${formatBytes(entry.done)} of ${formatBytes(entry.total)}`;
		case 'finishing':
			return 'Finishing';
		case 'done':
			if (entry.deduplicated) return 'Already on your drive';
			return entry.finalName && entry.finalName !== entry.name ? `Saved as ${entry.finalName}` : 'Done';
		case 'failed':
			return entry.error ?? 'Upload failed';
		case 'cancelled':
			return 'Cancelled';
	}
}
