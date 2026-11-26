<script lang="ts" module>
	import type { IconName } from '#lib/components/icons/paths.ts';
	import type { FaceCrop } from '#lib/explore/faces.ts';

	export interface RuleChip {
		key: string;
		label: string;
		/** A person's face; otherwise `icon` leads. */
		face?: FaceCrop | null;
		icon?: IconName;
	}
</script>

<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import FaceAvatar from '#lib/components/photos/FaceAvatar.svelte';

	interface Props {
		chips: readonly RuleChip[];
		/** Names the remove buttons: "Remove person Alice". */
		subject: string;
		onRemove: (key: string) => void;
	}

	let { chips, subject, onRemove }: Props = $props();
</script>

<!-- MASTER "Filter chip": chosen segments with a clear button, as in the photo filters. -->
<ul class="chips">
	{#each chips as chip (chip.key)}
		<li class="chip">
			{#if chip.face !== undefined}<FaceAvatar face={chip.face} />{:else}<Icon name={chip.icon ?? 'tag'} />{/if}
			<span class="label">{chip.label}</span>
			<button type="button" class="clear" aria-label={`Remove ${subject} ${chip.label}`} title="Remove" onclick={() => onRemove(chip.key)}>
				<Icon name="x" />
			</button>
		</li>
	{/each}
</ul>

<style>
	.chips {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		min-width: 0;
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.chip {
		display: inline-flex;
		align-items: center;
		gap: var(--space-2);
		min-height: var(--chip-h);
		max-width: 100%;
		padding: 0 var(--space-1) 0 var(--space-3);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-knob);
		color: var(--ink);
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
		font-weight: var(--weight-label);
		letter-spacing: var(--track-label);
	}

	.chip:has(:global(.avatar)) {
		padding-left: var(--space-1);
	}

	.chip :global(.avatar) {
		width: calc(var(--chip-h) - 2 * var(--space-1));
		height: calc(var(--chip-h) - 2 * var(--space-1));
	}

	.label {
		overflow: hidden;
		text-overflow: ellipsis;
		white-space: nowrap;
	}

	.clear {
		position: relative;
		display: inline-grid;
		place-items: center;
		width: var(--chip-h);
		height: calc(var(--chip-h) - 2 * var(--space-1));
		padding: 0;
		border: 0;
		border-radius: var(--radius-full);
		background: transparent;
		color: var(--ink-muted);
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			color var(--dur-fast) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.clear :global(.icon) {
		width: var(--chip-clear);
		height: var(--chip-clear);
	}

	.clear::before {
		content: '';
		position: absolute;
		inset: calc((var(--chip-h) - var(--tap)) / 2);
	}

	@media (hover: hover) {
		.clear:hover {
			background: var(--tint-strong);
			color: var(--ink);
		}
	}

	.clear:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.clear:active {
		transform: translateY(var(--press));
	}

	@media (forced-colors: active) {
		.chip {
			border: 1px solid CanvasText;
		}
	}
</style>
