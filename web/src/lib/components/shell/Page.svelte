<script lang="ts">
	import type { Snippet } from 'svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';

	/** A sub-page's way back to its parent page (MASTER.md › "Page header"). */
	interface Back {
		href: string;
		/** The parent page's name: the button is named "Back to <label>". */
		label: string;
	}

	interface Props {
		/** Page heading (h1). */
		title: string;
		/** Shown above the heading, e.g. breadcrumbs. */
		above?: Snippet;
		/** Short muted line under the heading. */
		description?: Snippet;
		/** Page-level buttons next to the heading (below it on phones). */
		actions?: Snippet;
		/** Sub-pages: a back button before the heading, to the parent page. */
		back?: Back;
		children: Snippet;
	}

	let { title, above, description, actions, back, children }: Props = $props();
</script>

<main class="page">
	<header class="head arrive" style:--i={0}>
		{#if above}<div class="above">{@render above()}</div>{/if}
		<div class="title-row">
			<div class="titles">
				<div class="heading">
					{#if back}
						<IconButton icon="chevron-left" label={`Back to ${back.label}`} tooltip="Back" fill="tint" href={back.href} />
					{/if}
					<h1>{title}</h1>
				</div>
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

	.heading {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		min-width: 0;
	}

	h1 {
		min-width: 0;
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
