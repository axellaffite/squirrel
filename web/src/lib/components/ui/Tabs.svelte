<script lang="ts" generics="T extends string">
	/**
	 * Tabs over panels the page renders (WAI-ARIA tabs pattern). Drawn as MASTER's segmented control:
	 * a tinted pill track, the chosen tab a card pill. Arrow keys, Home and End move between tabs.
	 */
	interface Tab {
		value: T;
		label: string;
		/** Shown after the label (tabular), e.g. how many items the panel holds. */
		count?: number | null;
	}

	interface Props {
		/** The tab list's accessible name. */
		label: string;
		/** Prefix for the tab and panel ids: panels use `${id}-panel-${value}`. */
		id: string;
		tabs: readonly Tab[];
		value: T;
		onchange: (value: T) => void;
	}

	let { label, id, tabs, value, onchange }: Props = $props();

	let buttons: HTMLButtonElement[] = $state([]);

	function onKeydown(event: KeyboardEvent, index: number) {
		const last = tabs.length - 1;
		const next =
			event.key === 'ArrowRight' ? (index === last ? 0 : index + 1)
			: event.key === 'ArrowLeft' ? (index === 0 ? last : index - 1)
			: event.key === 'Home' ? 0
			: event.key === 'End' ? last
			: null;
		if (next === null) return;
		event.preventDefault();
		onchange(tabs[next].value);
		buttons[next]?.focus();
	}
</script>

<div class="tabs" role="tablist" aria-label={label}>
	{#each tabs as tab, index (tab.value)}
		{@const selected = tab.value === value}
		<button
			bind:this={buttons[index]}
			type="button"
			role="tab"
			id={`${id}-tab-${tab.value}`}
			aria-selected={selected}
			aria-controls={`${id}-panel-${tab.value}`}
			tabindex={selected ? 0 : -1}
			class:selected
			onclick={() => onchange(tab.value)}
			onkeydown={(event) => onKeydown(event, index)}
		>
			{tab.label}
			{#if tab.count != null}<span class="count">{tab.count}</span>{/if}
		</button>
	{/each}
</div>

<style>
	.tabs {
		display: inline-flex;
		justify-self: start;
		width: fit-content;
		max-width: 100%;
		gap: var(--space-1);
		padding: var(--space-1);
		border-radius: var(--radius-full);
		background: var(--tint);
		overflow-x: auto;
	}

	button {
		position: relative;
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--control-sm);
		padding: 0 var(--space-4);
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--ink-muted);
		font: inherit;
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
		white-space: nowrap;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease);
	}

	/* 44 px hit area around the 36 px segment. */
	button::before {
		content: '';
		position: absolute;
		inset: calc((var(--control-sm) - var(--tap)) / 2) 0;
	}

	@media (hover: hover) {
		button:not(.selected):hover {
			color: var(--ink);
		}
	}

	.selected {
		background: var(--card);
		color: var(--ink);
		box-shadow: var(--shadow-knob);
	}

	button:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.count {
		min-width: var(--icon-sm);
		padding: 0 var(--space-1);
		border-radius: var(--radius-full);
		background: var(--tint);
		color: var(--ink-muted);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-variant-numeric: tabular-nums;
		text-align: center;
	}

	@media (forced-colors: active) {
		.selected {
			outline: 1px solid Highlight;
		}
	}
</style>
