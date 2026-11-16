<script lang="ts">
	import { goto } from '$app/navigation';
	import Icon from '#lib/components/icons/Icon.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import { MAX_QUERY, normalizeQuery } from '#lib/files/search.ts';

	interface Props {
		value?: string;
		/** Called as the user types (the search page searches as you type). */
		oninput?: (value: string) => void;
		/** Defaults to opening `/files/search?q=…`. */
		onsubmit?: (value: string) => void;
		autofocus?: boolean;
	}

	let { value = $bindable(''), oninput, onsubmit, autofocus = false }: Props = $props();

	const uid = $props.id();
	const inputId = `search-${uid}`;
	let input: HTMLInputElement | undefined = $state();

	/** The native clear button is hidden (it cannot take the tokens), and Firefox has none. */
	function clear() {
		value = '';
		oninput?.('');
		input?.focus();
	}

	function submit(event: SubmitEvent) {
		event.preventDefault();
		const query = normalizeQuery(value);
		if (onsubmit) onsubmit(query);
		else if (query) void goto(`/files/search?q=${encodeURIComponent(query)}`);
	}
</script>

<form class="search" role="search" onsubmit={submit}>
	<label class="visually-hidden" for={inputId}>Search files</label>
	<Icon name="search" />
	<!-- svelte-ignore a11y_autofocus -->
	<input
		bind:this={input}
		id={inputId}
		type="search"
		bind:value
		placeholder="Search names and documents"
		maxlength={MAX_QUERY}
		autocomplete="off"
		spellcheck="false"
		enterkeyhint="search"
		{autofocus}
		oninput={() => oninput?.(value)}
	/>
	{#if value}
		<span class="clear"><IconButton icon="x" label="Clear the search" tooltip="Clear" onclick={clear} /></span>
	{/if}
</form>

<style>
	/* The text field pill with a leading search icon. */
	/* Full width on phones (its own line among the page actions); a fixed pill from 768 px. */
	.search {
		position: relative;
		display: flex;
		align-items: center;
		width: 100%;
	}

	@media (min-width: 48rem) {
		.search {
			width: var(--menu-w);
		}
	}

	.search :global(.icon) {
		position: absolute;
		left: var(--space-4);
		color: var(--ink-muted);
		pointer-events: none;
	}

	input {
		width: 100%;
		min-height: var(--control-md);
		padding: var(--space-2) var(--space-4) var(--space-2) calc(var(--space-4) + var(--icon-sm) + var(--space-2));
		border: 1px solid var(--edge);
		border-radius: var(--radius-full);
		background: var(--card);
		color: var(--ink);
		font-size: var(--text-md);
		line-height: var(--text-md-lh);
		transition: border-color var(--dur-fast) var(--ease);
	}

	/* The browser's own clear button cannot take the tokens: ours replaces it. */
	input::-webkit-search-cancel-button {
		appearance: none;
	}

	.clear {
		position: absolute;
		right: var(--space-1);
		display: grid;
	}

	/* Room for the clear button so text never runs under it. */
	input {
		padding-right: calc(var(--control-sm) + var(--space-2));
	}

	input::placeholder {
		color: var(--ink-muted);
	}

	@media (hover: hover) {
		input:hover {
			border-color: var(--ink-muted);
		}
	}

	input:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	@media (forced-colors: active) {
		input {
			border-color: CanvasText;
		}
	}
</style>
