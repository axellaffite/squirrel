<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		tone?: 'neutral' | 'accent' | 'success' | 'warning' | 'danger';
		children: Snippet;
	}

	let { tone = 'neutral', children }: Props = $props();
</script>

<span class="badge {tone}">{@render children()}</span>

<style>
	.badge {
		display: inline-flex;
		align-items: center;
		gap: var(--space-1);
		padding: var(--space-1) var(--space-2);
		border-radius: var(--radius-full);
		background: var(--tint);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		font-variant-numeric: tabular-nums;
		white-space: nowrap;
		color: var(--ink-muted);
	}

	.accent {
		color: var(--accent-ink);
	}

	.success {
		color: var(--success);
	}

	.warning {
		color: var(--warning);
	}

	.danger {
		color: var(--danger);
	}

	@media (forced-colors: active) {
		.badge {
			border: 1px solid CanvasText;
		}
	}
</style>
