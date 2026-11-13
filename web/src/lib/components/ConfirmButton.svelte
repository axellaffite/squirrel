<script lang="ts">
	let {
		label,
		confirmLabel,
		onConfirm,
		disabled = false
	}: { label: string; confirmLabel: string; onConfirm: () => Promise<void> | void; disabled?: boolean } = $props();
	let asking = $state(false);
	let busy = $state(false);

	async function confirm() {
		busy = true;
		try {
			await onConfirm();
		} finally {
			busy = false;
			asking = false;
		}
	}
</script>

{#if asking}
	<span class="confirm">
		<button class="danger-button" onclick={confirm} disabled={busy}>{confirmLabel}</button>
		<button class="link" onclick={() => (asking = false)}>Cancel</button>
	</span>
{:else}
	<button class="link danger" onclick={() => (asking = true)} {disabled}>{label}</button>
{/if}
