<script lang="ts">
	import type { Snippet } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';

	interface Props {
		tone?: 'danger' | 'success';
		/** Live region role; errors use "alert" so they are announced at once. */
		role?: 'alert' | 'status';
		children: Snippet;
	}

	let { tone = 'danger', role = tone === 'danger' ? 'alert' : 'status', children }: Props = $props();
</script>

<div class="alert {tone}" {role}>
	<Icon name={tone === 'danger' ? 'alert' : 'check'} />
	<div class="body">{@render children()}</div>
</div>

<style>
	.alert {
		display: flex;
		gap: var(--space-3);
		align-items: flex-start;
		padding: var(--space-3) var(--space-4);
		border-radius: var(--radius-md);
		box-shadow: var(--inset-1);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
	}

	.danger {
		background: var(--danger-soft);
		color: var(--danger);
	}

	.success {
		background: var(--surface);
		color: var(--success);
	}

	.body {
		min-width: 0;
		overflow-wrap: anywhere;
	}
</style>
