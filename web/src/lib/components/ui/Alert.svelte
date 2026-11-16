<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';

	interface Props {
		tone?: 'danger' | 'success' | 'info';
		/**
		 * Live region role; errors use "alert" so they are announced at once. "note" is not live:
		 * for messages announced elsewhere (e.g. once at start and end, not at every update).
		 */
		role?: 'alert' | 'status' | 'note';
		children: Snippet;
	}

	let { tone = 'danger', role = tone === 'danger' ? 'alert' : 'status', children }: Props = $props();
</script>

<div class="alert {tone}" {role}>
	<Icon name={tone === 'danger' ? 'alert' : tone === 'info' ? 'info' : 'check'} />
	<div class="body">{@render children()}</div>
</div>

<style>
	.alert {
		display: flex;
		gap: var(--space-3);
		align-items: flex-start;
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-card);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
	}

	.danger {
		background: var(--danger-soft);
		color: var(--danger);
	}

	.success {
		background: var(--tint);
		color: var(--success);
	}

	.info {
		background: var(--tint);
		color: var(--ink);
	}

	.body {
		min-width: 0;
		overflow-wrap: anywhere;
	}

	@media (forced-colors: active) {
		.alert {
			border: 1px solid CanvasText;
		}
	}
</style>
