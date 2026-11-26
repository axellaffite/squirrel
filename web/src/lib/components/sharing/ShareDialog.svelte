<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import FriendPicks from '#lib/components/sharing/FriendPicks.svelte';
	import ShareRow from '#lib/components/sharing/ShareRow.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { deleteShare, listFriends, listShares, shareWith, type Friend, type Permission, type Share, type Subject } from '#lib/sharing/api.ts';
	import { pickableFriends } from '#lib/sharing/friends.ts';
	import { removeById, upsertShare } from '#lib/sharing/logic.ts';

	interface Props {
		subject: Subject;
		/** The folder, file or album name, for the title. */
		name: string;
		/** Only "Read" can be given (a dynamic album: its photos come from rules). */
		readOnly?: boolean;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { subject, name, readOnly = false, onClose }: Props = $props();

	const PERMISSIONS = [
		{ value: 'read', label: 'Read' },
		{ value: 'write', label: 'Write' }
	] as const;

	const target = untrack(() => subject);
	let open = $state(true);
	let shares: Share[] | null = $state(null);
	/** Without friends (none, or they failed to load) the dialog works by username alone. */
	let friends: Friend[] = $state([]);
	const picks = $derived(pickableFriends(friends, shares));
	let username = $state('');
	let permission: Permission = $state('read');
	let error: string | null = $state(null);
	let busy = $state(false);
	const uid = $props.id();

	onMount(async () => {
		const [shared, known] = await Promise.all([listShares(target), listFriends()]);
		if (shared.ok) shares = shared.data;
		else error = shared.error;
		if (known.ok) friends = known.data.friends;
	});

	/** Shares with `who`; returns whether it worked (the error is shown otherwise). */
	async function share(who: string): Promise<boolean> {
		busy = true;
		error = null;
		const result = await shareWith(target, who, readOnly ? 'read' : permission);
		busy = false;
		if (!result.ok) {
			error = result.error;
			return false;
		}
		shares = upsertShare(shares ?? [], result.data);
		return true;
	}

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const who = username.trim();
		if (!who) {
			error = 'Enter a username.';
			return;
		}
		if (await share(who)) username = '';
	}

	async function change(share: Share, next: Permission) {
		error = null;
		const result = await shareWith(target, share.person.username, next);
		if (result.ok) shares = upsertShare(shares ?? [], result.data);
		else error = result.error;
	}

	async function stop(share: Share) {
		error = null;
		const result = await deleteShare(share.id);
		if (result.ok) shares = removeById(shares ?? [], share.id);
		else error = result.error;
	}
</script>

<Dialog bind:open title={`Share “${name}”`} onclose={onClose}>
	<div class="body">
		<form class="form" onsubmit={submit}>
			<TextField
				label="Username"
				bind:value={username}
				autocomplete="off"
				autocapitalize="none"
				spellcheck={false}
				hint="Someone with an account on this Squirrel."
			/>
			<div class="row">
				{#if readOnly}
					<p class="muted">People you share it with can view it. Its photos come from its rules, so nobody can add any.</p>
				{:else}
					<SegmentedControl label="Permission" name={`permission-${uid}`} options={PERMISSIONS} value={permission} onchange={(value) => (permission = value)} />
				{/if}
				<Button type="submit" disabled={busy}><Icon name="share" />Share</Button>
			</div>
		</form>
		<FriendPicks friends={picks} disabled={busy || shares === null} onPick={(friend) => share(friend.username)} />
		{#if error}<Alert>{error}</Alert>{/if}
		{#if shares === null}
			{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
		{:else if shares.length === 0}
			<p class="muted">Not shared with anyone yet.</p>
		{:else}
			<ul class="shares" aria-label="People with access">
				{#each shares as share (share.id)}
					<ShareRow {share} {readOnly} onChange={(next) => change(share, next)} onStop={() => stop(share)} />
				{/each}
			</ul>
		{/if}
	</div>
</Dialog>

<style>
	.body,
	.form {
		display: grid;
		gap: var(--space-4);
	}

	.row {
		display: flex;
		flex-wrap: wrap;
		align-items: center;
		justify-content: space-between;
		gap: var(--space-3);
	}

	.shares {
		display: grid;
		gap: var(--space-2);
		margin: 0;
		padding: var(--space-2);
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}
</style>
