<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import type { Friend } from '#lib/sharing/api.ts';

	interface Props {
		/** Friends who do not have access yet. */
		friends: Friend[];
		disabled: boolean;
		onPick: (friend: Friend) => void;
	}

	let { friends, disabled, onPick }: Props = $props();
	const uid = $props.id();
</script>

{#if friends.length > 0}
	<div class="picks">
		<span id={`friend-picks-${uid}`} class="label">Friends</span>
		<ul aria-labelledby={`friend-picks-${uid}`}>
			{#each friends as friend (friend.username)}
				<li>
					<Button variant="secondary" size="sm" {disabled} aria-label={`Share with ${friend.display_name}`} onclick={() => onPick(friend)}
						><Icon name="user" />{friend.display_name}</Button
					>
				</li>
			{/each}
		</ul>
	</div>
{/if}

<style>
	.picks {
		display: grid;
		gap: var(--space-2);
	}

	.label {
		font-size: var(--text-sm);
		font-weight: var(--weight-label);
	}

	ul {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
