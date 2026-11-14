<script lang="ts">
	import type { HTMLInputAttributes } from 'svelte/elements';
	import Icon from '#lib/components/icons/Icon.svelte';

	interface Props extends Omit<HTMLInputAttributes, 'value'> {
		label: string;
		value?: string;
		/** Help text under the field, linked with aria-describedby. */
		hint?: string | null;
		/** Error under the field; marks the input aria-invalid. */
		error?: string | null;
	}

	let { label, value = $bindable(''), hint = null, error = null, id, type = 'text', ...rest }: Props = $props();

	const uid = $props.id();
	const inputId = $derived(id ?? `field-${uid}`);
	const hintId = $derived(`${inputId}-hint`);
	const errorId = $derived(`${inputId}-error`);
	const describedBy = $derived([hint ? hintId : null, error ? errorId : null].filter(Boolean).join(' ') || undefined);
</script>

<div class="field">
	<label for={inputId}>{label}</label>
	<input
		id={inputId}
		{type}
		bind:value
		aria-describedby={describedBy}
		aria-invalid={error ? 'true' : undefined}
		{...rest}
	/>
	{#if hint}<p id={hintId} class="hint">{hint}</p>{/if}
	{#if error}
		<p id={errorId} class="message"><Icon name="alert" />{error}</p>
	{/if}
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

	/* Pill field: card fill with a 1 px edge of at least 3:1. */
	input {
		width: 100%;
		min-height: var(--control-md);
		padding: var(--space-2) var(--space-4);
		border: 1px solid var(--edge);
		border-radius: var(--radius-full);
		background: var(--card);
		color: var(--ink);
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		font-weight: var(--weight-body);
		transition:
			border-color var(--dur-fast) var(--ease),
			background-color var(--dur-fast) var(--ease);
	}

	input::placeholder {
		color: var(--ink-muted);
	}

	@media (hover: hover) {
		input:not(:disabled):hover {
			border-color: var(--ink-muted);
		}
	}

	input:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
		border-color: var(--ink-muted);
	}

	input:active:not(:disabled) {
		border-color: var(--ink);
	}

	input:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	input[aria-invalid='true'] {
		border-color: var(--danger);
	}

	.hint,
	.message {
		padding-inline: var(--space-1);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	.hint {
		color: var(--ink-muted);
	}

	.message {
		display: flex;
		gap: var(--space-2);
		align-items: flex-start;
		color: var(--danger);
		font-weight: var(--weight-label);
	}
</style>
