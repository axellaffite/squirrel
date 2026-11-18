<script lang="ts">
	interface Props {
		label: string;
		checked?: boolean;
		disabled?: boolean;
		onchange?: (checked: boolean) => void;
		/** The parent owns the state: a click only asks for the change (through `onchange`). */
		controlled?: boolean;
	}

	let { label, checked = $bindable(false), disabled = false, onchange, controlled = false }: Props = $props();

	function toggle() {
		const next = !checked;
		if (!controlled) checked = next;
		onchange?.(next);
	}
</script>

<button type="button" role="switch" class="toggle" aria-checked={checked} {disabled} onclick={toggle}>
	<span class="label">{label}</span>
	<span class="track" aria-hidden="true"><span class="knob"></span></span>
</button>

<style>
	.toggle {
		display: inline-flex;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
		min-height: var(--tap);
		padding: 0;
		border: 0;
		background: none;
		color: var(--ink);
		font-size: var(--text-md);
		cursor: pointer;
	}

	.track {
		/* Inner box 40 × 24, knob 18: built from spacing steps. Content-box keeps the 1 px edge outside. */
		--track-w: calc(var(--space-6) + var(--space-2));
		--knob: calc(var(--space-4) + var(--space-1) / 2);
		position: relative;
		flex: none;
		box-sizing: content-box;
		width: var(--track-w);
		height: var(--space-5);
		border: 1px solid var(--edge);
		border-radius: var(--radius-full);
		background: var(--tint);
		transition:
			background-color var(--dur-fast) var(--ease),
			border-color var(--dur-fast) var(--ease);
	}

	.knob {
		position: absolute;
		top: 50%;
		left: var(--space-1);
		width: var(--knob);
		height: var(--knob);
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-knob);
		transform: translateY(-50%);
		transition: transform var(--dur-fast) var(--ease);
	}

	[aria-checked='true'] .track {
		background: var(--accent);
		border-color: var(--accent);
	}

	[aria-checked='true'] .knob {
		background: var(--on-accent);
		transform: translate(calc(var(--track-w) - var(--knob) - 2 * var(--space-1)), -50%);
	}

	@media (hover: hover) {
		.toggle:not(:disabled):hover .track {
			border-color: var(--ink-muted);
		}
	}

	.toggle:focus-visible {
		outline: none;
	}

	.toggle:focus-visible .track {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.toggle:not(:disabled):active .knob {
		transform: translate(0, calc(-50% + var(--press)));
	}

	[aria-checked='true'].toggle:not(:disabled):active .knob {
		transform: translate(calc(var(--track-w) - var(--knob) - 2 * var(--space-1)), calc(-50% + var(--press)));
	}

	.toggle:disabled {
		opacity: var(--disabled-opacity);
		cursor: not-allowed;
	}

	/* Forced colours drop the fills: keep the knob and the checked track visible. */
	@media (forced-colors: active) {
		.knob {
			background: CanvasText;
		}

		[aria-checked='true'] .track {
			background: Highlight;
		}

		[aria-checked='true'] .knob {
			background: Canvas;
		}
	}
</style>
