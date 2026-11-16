<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';

	interface Props {
		count: number;
		busy?: boolean;
		onDelete: () => void;
		onClear: () => void;
	}

	let { count, busy = false, onDelete, onClear }: Props = $props();
</script>

<!-- Floats above the phone rail; from 1024 px at the top of the panel. -->
<div class="dock">
	<div class="bar" role="toolbar" aria-label="Selected photos">
		<p class="count tabular" aria-live="polite">{count} selected</p>
		<Button variant="danger" size="sm" onclick={onDelete} disabled={busy}>
			<Icon name="trash" />Delete
		</Button>
		<IconButton icon="x" label="Clear the selection" tooltip="Clear" onclick={onClear} />
	</div>
</div>

<style>
	.dock {
		position: fixed;
		z-index: 20;
		left: var(--frame);
		right: var(--frame);
		bottom: calc(var(--rail-bar-h) + var(--space-5) + env(safe-area-inset-bottom));
		display: flex;
		justify-content: center;
		pointer-events: none;
	}

	.bar {
		display: flex;
		align-items: center;
		gap: var(--space-2);
		padding: var(--space-1) var(--space-1) var(--space-1) var(--space-4);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-pop);
		pointer-events: auto;
	}

	.count {
		margin-right: var(--space-2);
		font-weight: var(--weight-label);
		white-space: nowrap;
	}

	@media (min-width: 64rem) {
		.dock {
			top: calc(var(--frame) + var(--space-4));
			bottom: auto;
			left: calc(var(--frame) + var(--rail-w) + var(--space-4));
		}
	}

	@media (forced-colors: active) {
		.bar {
			border: 1px solid CanvasText;
		}
	}
</style>
