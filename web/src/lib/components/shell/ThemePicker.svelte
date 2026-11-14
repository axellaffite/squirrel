<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import { THEME_CHOICES, type ThemeChoice } from '#lib/theme.ts';

	interface Props {
		value: ThemeChoice;
		onchange: (choice: ThemeChoice) => void;
	}

	let { value, onchange }: Props = $props();

	const LABELS: Record<ThemeChoice, { label: string; icon: IconName }> = {
		system: { label: 'System', icon: 'monitor' },
		light: { label: 'Light', icon: 'sun' },
		dark: { label: 'Dark', icon: 'moon' }
	};
</script>

<fieldset>
	<legend>Theme</legend>
	<div class="segments">
		{#each THEME_CHOICES as choice (choice)}
			<label class:checked={value === choice}>
				<input
					type="radio"
					name="theme"
					value={choice}
					checked={value === choice}
					onchange={() => onchange(choice)}
				/>
				<Icon name={LABELS[choice].icon} />
				<span>{LABELS[choice].label}</span>
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

	legend {
		padding: 0;
		margin-bottom: var(--space-2);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		letter-spacing: 0.04em;
		text-transform: uppercase;
		color: var(--ink-muted);
	}

	.segments {
		display: grid;
		grid-template-columns: repeat(3, 1fr);
		gap: var(--space-1);
		padding: var(--space-1);
		border-radius: var(--radius-md);
		box-shadow: var(--inset-1);
	}

	label {
		position: relative;
		display: grid;
		justify-items: center;
		gap: var(--space-1);
		min-height: var(--tap);
		padding: var(--space-2) var(--space-1);
		border-radius: var(--radius-sm);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-label);
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			color var(--dur-fast) var(--ease),
			box-shadow var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	input {
		position: absolute;
		opacity: 0;
		inset: 0;
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
		color: var(--accent);
		background: var(--surface);
		box-shadow: var(--raise-1);
	}

	label:has(input:disabled) {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}
</style>
