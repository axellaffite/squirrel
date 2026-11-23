<script lang="ts">
	import { onMount, untrack } from 'svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import ShareRow from '#lib/components/sharing/ShareRow.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import TextField from '#lib/components/ui/TextField.svelte';
	import { deleteShare, listShares, shareWith, type Permission, type Share, type Subject } from '#lib/sharing/api.ts';
	import { removeById, upsertShare } from '#lib/sharing/logic.ts';

	interface Props {
		subject: Subject;
		/** The folder, file or album name, for the title. */
		name: string;
		/** Runs once the dialog closed, however it closed. */
		onClose: () => void;
	}

	let { subject, name, onClose }: Props = $props();

	const PERMISSIONS = [
		{ value: 'read', label: 'Read' },
		{ value: 'write', label: 'Write' }
	] as const;

	const target = untrack(() => subject);
	let open = $state(true);
	let shares: Share[] | null = $state(null);
	let username = $state('');
	let permission: Permission = $state('read');
	let error: string | null = $state(null);
	let busy = $state(false);
	const uid = $props.id();

	onMount(async () => {
		const result = await listShares(target);
		if (result.ok) shares = result.data;
		else error = result.error;
	});

	async function submit(event: SubmitEvent) {
		event.preventDefault();
		const who = username.trim();
		if (!who) {
			error = 'Enter a username.';
			return;
		}
		busy = true;
		error = null;
		const result = await shareWith(target, who, permission);
		busy = false;
		if (!result.ok) {
			error = result.error;
			return;
		}
		shares = upsertShare(shares ?? [], result.data);
		username = '';
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
				<SegmentedControl label="Permission" name={`permission-${uid}`} options={PERMISSIONS} value={permission} onchange={(value) => (permission = value)} />
				<Button type="submit" disabled={busy}><Icon name="share" />Share</Button>
			</div>
		</form>
		{#if error}<Alert>{error}</Alert>{/if}
		{#if shares === null}
			{#if !error}<p class="muted" aria-live="polite">Loading…</p>{/if}
		{:else if shares.length === 0}
			<p class="muted">Not shared with anyone yet.</p>
		{:else}
			<ul class="shares" aria-label="People with access">
				{#each shares as share (share.id)}
					<ShareRow {share} onChange={(next) => change(share, next)} onStop={() => stop(share)} />
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
