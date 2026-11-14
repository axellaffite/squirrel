<script lang="ts">
	import { ICONS, type IconName } from './paths.ts';

	interface Props {
		name: IconName;
		/** 20 px (default) or 24 px. */
		size?: 20 | 24;
		/** Accessible name. Without it the icon is decorative and hidden from assistive tech. */
		label?: string;
		class?: string;
	}

	let { name, size = 20, label, class: className = '' }: Props = $props();
</script>

<svg
	class="icon {className}"
	class:md={size === 24}
	viewBox="0 0 24 24"
	fill="none"
	stroke="currentColor"
	stroke-width="1.75"
	stroke-linecap="round"
	stroke-linejoin="round"
	aria-hidden={label ? undefined : 'true'}
	role={label ? 'img' : undefined}
	aria-label={label}
	focusable="false"
>
	{#each ICONS[name] as d (d)}<path {d} />{/each}
</svg>

<style>
	.icon {
		width: var(--icon-sm);
		height: var(--icon-sm);
		flex: none;
	}

	.md {
		width: var(--icon-md);
		height: var(--icon-md);
	}
</style>
