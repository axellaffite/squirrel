<script lang="ts">
	import type { HTMLTextareaAttributes } from 'svelte/elements';
	import Icon from '#lib/components/icons/Icon.svelte';

	/** The text field for several lines (comments): card fill, 1 px edge, card radius. */
	interface Props extends Omit<HTMLTextareaAttributes, 'value'> {
		label: string;
		/** Hide the label visually (it still names the field). */
		hideLabel?: boolean;
		value?: string;
		error?: string | null;
		element?: HTMLTextAreaElement;
	}

	let { label, hideLabel = false, value = $bindable(''), error = null, element = $bindable(), id, rows = 3, ...rest }: Props = $props();

	const uid = $props.id();
	const inputId = $derived(id ?? `area-${uid}`);
	const errorId = $derived(`${inputId}-error`);
</script>

<div class="field">
	<label for={inputId} class:visually-hidden={hideLabel}>{label}</label>
	<textarea
		bind:this={element}
		id={inputId}
		{rows}
		bind:value
		aria-describedby={error ? errorId : undefined}
		aria-invalid={error ? 'true' : undefined}
		{...rest}
	></textarea>
	{#if error}<p id={errorId} class="message"><Icon name="alert" />{error}</p>{/if}
</div>

<style>
	.field {
		display: grid;
		gap: var(--space-2);
	}

	label {
		display: block;
		padding-inline: var(--space-1);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		color: var(--ink);
	}

	textarea {
		width: 100%;
		min-height: var(--control-md);
		padding: var(--space-2) var(--space-4);
		border: 1px solid var(--edge);
		border-radius: var(--radius-card);
		background: var(--card);
		color: var(--ink);
		font: inherit;
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		resize: vertical;
		transition:
			border-color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease);
	}

	textarea::placeholder {
		color: var(--ink-muted);
	}

	@media (hover: hover) {
		textarea:not(:disabled):hover {
			border-color: var(--ink-muted);
		}
	}

	textarea:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
		border-color: var(--ink-muted);
	}

	textarea:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	textarea[aria-invalid='true'] {
		border-color: var(--danger);
	}

	.message {
		display: flex;
		gap: var(--space-2);
		align-items: flex-start;
		padding-inline: var(--space-1);
		color: var(--danger);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
	}
</style>
