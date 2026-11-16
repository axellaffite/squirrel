<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';

	interface Props {
		name: string;
		onRename: () => void;
		onDelete: () => void;
	}

	let { name, onRename, onDelete }: Props = $props();

	let sheet = $state(false);
	let dialog: Dialog | undefined = $state();
	/** The chosen action runs once the sheet closed, so the next dialog starts clean. */
	let chosen: (() => void) | null = null;

	function choose(run: () => void) {
		chosen = run;
		dialog?.close();
	}

	function closed() {
		const run = chosen;
		chosen = null;
		run?.();
	}
</script>

<!-- Inline from 768 px; one "Actions" button opening a sheet on phones, like file rows. -->
<span class="inline">
	<Button variant="secondary" onclick={onRename}><Icon name="pencil" />Rename</Button>
	<Button variant="danger" onclick={onDelete}><Icon name="trash" />Delete album</Button>
</span>
<span class="more">
	<IconButton icon="more" label={`Actions for the album ${name}`} tooltip="Actions" fill="tint" onclick={() => (sheet = true)} />
</span>

{#if sheet}
	<Dialog bind:this={dialog} bind:open={sheet} title={name} onclose={closed}>
		<ul class="actions">
			<li>
				<button type="button" class="action" onclick={() => choose(onRename)}>
					<span class="well"><Icon name="pencil" /></span>Rename
				</button>
			</li>
			<li>
				<button type="button" class="action danger" onclick={() => choose(onDelete)}>
					<span class="well"><Icon name="trash" /></span>Delete album
				</button>
			</li>
		</ul>
	</Dialog>
{/if}

<style>
	.inline {
		display: none;
	}

	.more {
		display: contents;
	}

	@media (min-width: 48rem) {
		.inline {
			display: flex;
			gap: var(--space-2);
		}

		.more {
			display: none;
		}
	}

	.actions {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: var(--space-2);
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.action {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--ink);
		font-size: var(--text-md);
		font-weight: var(--weight-label);
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.danger {
		color: var(--danger);
	}

	.well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	@media (hover: hover) {
		.action:hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.action:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.action:active {
		transform: translateY(var(--press));
	}
</style>
