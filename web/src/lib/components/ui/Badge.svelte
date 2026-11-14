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
		background: var(--surface);
		box-shadow: var(--raise-1);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		letter-spacing: 0.04em;
		text-transform: uppercase;
		font-variant-numeric: tabular-nums;
		color: var(--ink-muted);
	}

	.accent {
		color: var(--accent);
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
</style>
