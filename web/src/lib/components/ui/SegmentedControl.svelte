<script lang="ts" generics="T extends string">
	/** MASTER segmented control: a tinted pill track, the chosen segment a card pill. A radio group. */
	interface Option {
		value: T;
		label: string;
	}

	interface Props {
		/** The group's accessible name (visually hidden). */
		label: string;
		/** Radio `name`, unique on the page. */
		name: string;
		options: readonly Option[];
		value: T;
		onchange: (value: T) => void;
	}

	let { label, name, options, value, onchange }: Props = $props();
</script>

<fieldset>
	<legend class="visually-hidden">{label}</legend>
	<div class="segments">
		{#each options as option (option.value)}
			<label class:checked={value === option.value}>
				<input type="radio" {name} value={option.value} checked={value === option.value} onchange={() => onchange(option.value)} />
				<span>{option.label}</span>
			</label>
		{/each}
	</div>
</fieldset>

<style>
	fieldset {
		margin: 0;
		padding: 0;
		border: 0;
		min-width: 0;
	}

	.segments {
		display: inline-flex;
		gap: var(--space-1);
		padding: var(--space-1);
		border-radius: var(--radius-full);
		background: var(--tint);
	}

	label {
		position: relative;
		display: flex;
		align-items: center;
		justify-content: center;
		min-height: var(--control-sm);
		padding: 0 var(--space-4);
		border-radius: var(--radius-full);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	/* The 44 px hit area around a small segment. */
	input {
		position: absolute;
		opacity: 0;
		inset: calc(-1 * var(--space-1)) 0;
		margin: 0;
		cursor: pointer;
	}

	@media (hover: hover) {
		label:hover {
			color: var(--ink);
		}
	}

	label:has(input:focus-visible) {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	label:active {
		transform: translateY(var(--press));
	}

	.checked {
		color: var(--ink);
		background: var(--card);
		box-shadow: var(--shadow-knob);
	}

	@media (forced-colors: active) {
		.checked {
			outline: var(--ring-width) solid Highlight;
		}
	}
</style>
