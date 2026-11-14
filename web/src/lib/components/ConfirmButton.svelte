<script lang="ts">
	import { onDestroy, tick } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';

	/** An armed confirmation falls back to the trigger after this long without a decision. */
	const DISARM_AFTER_MS = 8000;

	interface Props {
		label: string;
		confirmLabel: string;
		onConfirm: () => Promise<void> | void;
		disabled?: boolean;
		/** Icon-only trigger (rows); the label becomes its accessible name. */
		icon?: IconName;
		/** What the action applies to, appended to accessible names, e.g. a file name. */
		subject?: string;
	}

	let { label, confirmLabel, onConfirm, disabled = false, icon, subject }: Props = $props();

	let asking = $state(false);
	let busy = $state(false);
	let trigger: HTMLButtonElement | HTMLAnchorElement | undefined = $state();
	let confirmButton: HTMLButtonElement | HTMLAnchorElement | undefined = $state();
	let group: HTMLElement | undefined = $state();
	let timer: ReturnType<typeof setTimeout> | undefined;

	const named = (text: string) => (subject ? `${text} ${subject}` : text);

	async function arm() {
		asking = true;
		await tick();
		confirmButton?.focus();
		timer = setTimeout(() => disarm(group?.contains(document.activeElement) ?? false), DISARM_AFTER_MS);
	}

	/** Back to the trigger; focus follows only if it was inside the confirmation. */
	async function disarm(restoreFocus: boolean) {
		clearTimeout(timer);
		asking = false;
		if (!restoreFocus) return;
		await tick();
		trigger?.focus();
	}

	async function confirm() {
		clearTimeout(timer);
		busy = true;
		try {
			await onConfirm();
		} finally {
			busy = false;
			asking = false;
		}
	}

	function keydown(event: KeyboardEvent) {
		if (event.key !== 'Escape') return;
		event.preventDefault();
		void disarm(true);
	}

	onDestroy(() => clearTimeout(timer));
</script>

{#if asking}
	<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
	<span class="confirm" role="group" aria-label={named(label)} bind:this={group} onkeydown={keydown}>
		<Button
			variant="danger-solid"
			size="sm"
			bind:element={confirmButton}
			onclick={confirm}
			disabled={busy}
			aria-label={subject ? named(confirmLabel) : undefined}>{confirmLabel}</Button
		>
		<Button variant="quiet" size="sm" onclick={() => disarm(true)}>Cancel</Button>
	</span>
{:else if icon}
	<IconButton {icon} tone="danger" label={named(label)} tooltip={label} bind:element={trigger} onclick={arm} {disabled} />
{:else}
	<Button variant="danger" bind:element={trigger} onclick={arm} {disabled}><Icon name="trash" />{label}</Button>
{/if}

<style>
	.confirm {
		display: inline-flex;
		flex-wrap: wrap;
		align-items: center;
		gap: var(--space-2);
	}
</style>
