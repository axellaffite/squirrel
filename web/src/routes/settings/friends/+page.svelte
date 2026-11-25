<script lang="ts">
	import { onMount } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { addFriend, listFriends, removeFriend, type Friend, type Member } from '#lib/sharing/api.ts';
	import { addToFriends, withoutPerson } from '#lib/sharing/friends.ts';

	let friends: Friend[] | null = $state(null);
	let suggestions: Member[] = $state([]);
	let username = $state('');
	let error: string | null = $state(null);
	let busy = $state(false);

	async function load() {
		const result = await listFriends();
		if (!result.ok) {
			error = result.error;
			return;
		}
		friends = result.data.friends;
		suggestions = result.data.suggestions;
	}

	/** Adds `who`; returns whether it worked (the error is shown otherwise). */
	async function add(who: string): Promise<boolean> {
		busy = true;
		error = null;
		const result = await addFriend(who);
		busy = false;
		if (!result.ok) {
			error = result.error;
			return false;
		}
		friends = addToFriends(friends ?? [], result.data);
		suggestions = withoutPerson(suggestions, result.data.username);
		return true;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const who = username.trim();
		if (!who) {
			error = 'Enter a username.';
			return;
		}
		if (await add(who)) username = '';
	}

	/** Busy until the reload ends, so no add or second remove races it. */
	async function remove(friend: Friend) {
		busy = true;
		error = null;
		const result = await removeFriend(friend.username);
		if (result.ok) {
			friends = withoutPerson(friends ?? [], friend.username);
			// Someone you share with comes back as a suggestion: only the server knows.
			await load();
		} else error = result.error;
		busy = false;
	}

	onMount(load);
</script>

<svelte:head><title>Friends · Squirrel</title></svelte:head>

<Page title="Friends">
	{#snippet description()}People you share with often. They show up first when you share, and are not told when you add
		them.{/snippet}

	<form class="add" onsubmit={submit}>
		<div class="field">
			<TextField
				label="Username"
				bind:value={username}
				autocomplete="off"
				autocapitalize="none"
				spellcheck={false}
			/>
		</div>
		<Button type="submit" disabled={busy}><Icon name="user-plus" />Add friend</Button>
	</form>

	{#if error}<Alert>{error}</Alert>{/if}

	{#if friends === null}
		{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
	{:else if friends.length === 0}
		<EmptyState icon="users" title="No friends yet">Add the people you share with to reach them in one tap.</EmptyState>
	{:else}
		<ul class="rows" aria-label="Friends">
			{#each friends as friend (friend.username)}
				<ListRow>
					{#snippet icon()}<Icon name="user" />{/snippet}
					{#snippet title()}{friend.display_name}{/snippet}
					{#snippet meta()}@{friend.username}{/snippet}
					{#snippet actions()}
						<Button
							variant="quiet"
							size="sm"
							disabled={busy}
							aria-label={`Remove ${friend.display_name} from friends`}
							onclick={() => remove(friend)}
							>Remove</Button
						>
					{/snippet}
				</ListRow>
			{/each}
		</ul>
	{/if}

	{#if suggestions.length > 0}
		<section class="group" aria-labelledby="friend-suggestions">
			<h2 id="friend-suggestions">Suggestions</h2>
			<p class="muted">People you shared with, or who shared with you.</p>
			<ul class="rows" aria-label="Suggested friends">
				{#each suggestions as person (person.username)}
					<ListRow>
						{#snippet icon()}<Icon name="user" />{/snippet}
						{#snippet title()}{person.display_name}{/snippet}
						{#snippet meta()}@{person.username}{/snippet}
						{#snippet actions()}
							<Button
								variant="secondary"
								size="sm"
								disabled={busy}
								aria-label={`Add ${person.display_name} to friends`}
								onclick={() => add(person.username)}><Icon name="plus" />Add</Button
							>
						{/snippet}
					</ListRow>
				{/each}
			</ul>
		</section>
	{/if}
</Page>

<style>
	.add {
		display: grid;
		gap: var(--space-3);
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	@media (min-width: 48rem) {
		.add {
			grid-template-columns: minmax(0, 1fr) auto;
			align-items: end;
		}
	}

	.field {
		min-width: 0;
	}

	.group {
		display: grid;
		gap: var(--space-3);
	}

	h2 {
		font-size: var(--text-lg);
		line-height: var(--text-lg-lh);
		font-weight: var(--weight-heading);
		letter-spacing: var(--track-heading);
	}

	.rows {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: 0;
		list-style: none;
	}
</style>
