<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		/** Page heading (h1). */
		title: string;
		/** Shown above the heading, e.g. breadcrumbs. */
		above?: Snippet;
		/** Short muted line under the heading. */
		description?: Snippet;
		/** Page-level buttons next to the heading (below it on phones). */
		actions?: Snippet;
		children: Snippet;
	}

	let { title, above, description, actions, children }: Props = $props();
</script>

<main class="page">
	<header class="head arrive" style:--i={0}>
		{#if above}<div class="above">{@render above()}</div>{/if}
		<div class="title-row">
			<div class="titles">
				<h1>{title}</h1>
				{#if description}<p class="description">{@render description()}</p>{/if}
			</div>
			{#if actions}<div class="actions">{@render actions()}</div>{/if}
		</div>
	</header>
	{@render children()}
</main>

<style>
	.page {
		display: grid;
		gap: var(--space-5);
		align-content: start;
		min-width: 0;
	}

	.head {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	.title-row {
		display: grid;
		gap: var(--space-4);
	}

	.titles {
		display: grid;
		gap: var(--space-1);
		min-width: 0;
	}

	h1 {
		overflow-wrap: anywhere;
	}

	.description {
		color: var(--ink-muted);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.actions {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	/* Phones: actions share the width under the heading. */
	.actions > :global(*) {
		flex: 1 1 auto;
	}

	@media (min-width: 48rem) {
		.title-row {
			display: flex;
			align-items: flex-end;
			justify-content: space-between;
		}

		.actions {
			flex: none;
		}

		.actions > :global(*) {
			flex: none;
		}
	}
</style>
