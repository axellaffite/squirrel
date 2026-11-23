<script lang="ts">
	import { page } from '$app/state';
	import { onMount } from 'svelte';
	import Breadcrumbs from '#lib/components/files/Breadcrumbs.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import { openCollab } from '#lib/editor/api.ts';
	import type { CollabSession } from '#lib/editor/collab.ts';
	import {
		endMessage,
		openMessage,
		presenceUser,
		SAVE_QUIET_MS,
		statusLabel,
		type Person,
		type Phase
	} from '#lib/editor/logic.ts';
	import { getNode, type FileNode, type PathSegment } from '#lib/files/api.ts';
	import { contentUrl } from '#lib/files/format.ts';
	import { loadSession, session } from '#lib/session.svelte.ts';
	import type { Access } from '#lib/sharing/api.ts';

	/** A stale epoch reopens the document; more than this many in a row means something is off. */
	const MAX_REOPENS = 3;

	const id = $derived(page.params.id ?? '');

	let node: FileNode | null = $state(null);
	let path: PathSegment[] = $state([]);
	let access: Access = $state('read');
	let canWrite = $state(false);
	let phase: Phase = $state('connecting');
	let people: Person[] = $state([]);
	let changed = $state(false);
	let pending = $state(false);
	let error: string | null = $state(null);
	let notice: string | null = $state(null);
	let host: HTMLDivElement | undefined = $state();

	let collab: CollabSession | null = null;
	let quietTimer: ReturnType<typeof setTimeout> | undefined;
	let reopens = 0;
	let disposed = false;

	const status = $derived(statusLabel({ phase, changed, pending }));
	const others = $derived(people.filter((person) => !person.self).length);

	function stop() {
		collab?.destroy();
		collab = null;
		clearTimeout(quietTimer);
	}

	function changedNow() {
		changed = true;
		pending = true;
		clearTimeout(quietTimer);
		quietTimer = setTimeout(() => (pending = false), SAVE_QUIET_MS);
	}

	/** Opens (or reopens, after a stale epoch) a fresh document for the file. */
	async function connect(fileId: string, name: string) {
		const me = session.me;
		if (!me) return;
		phase = 'connecting';
		const [opened, module] = await Promise.all([openCollab(fileId), import('#lib/editor/collab.ts')]);
		if (disposed) return;
		if (!opened.ok) {
			phase = 'ended';
			error = openMessage(opened.problem);
			if (opened.problem === 'signed-out') void loadSession();
			return;
		}
		if (!host) return;
		canWrite = opened.ticket.can_write;
		changed = false;
		pending = false;
		collab = module.startCollab({
			parent: host,
			nodeId: fileId,
			fileName: name,
			ticket: opened.ticket,
			user: presenceUser(me.id, me.display_name),
			onPhase: (next) => (phase = next),
			// Only a synced document proves the reopen worked: the server refuses a
			// stale epoch after the socket opened, so 'connected' alone would loop.
			onSynced: () => (reopens = 0),
			onPeople: (next) => (people = next),
			onChange: changedNow,
			onLimit: () => (notice = endMessage('too-large')),
			onClosed: (outcome) => {
				stop();
				people = [];
				if (outcome.kind === 'reopen' && reopens < MAX_REOPENS) {
					reopens += 1;
					notice = 'The file or your access to it changed: showing the latest version.';
					void connect(fileId, name);
					return;
				}
				error = outcome.kind === 'end' ? endMessage(outcome.reason) : endMessage('refused');
				if (outcome.kind === 'end' && outcome.reason === 'signed-out') void loadSession();
			}
		});
	}

	async function load(fileId: string) {
		const details = await getNode(fileId);
		if (disposed) return;
		if (!details.ok || details.data.node.kind !== 'file') {
			phase = 'ended';
			error = !details.ok && details.status !== 404 ? details.error : openMessage('not-found');
			return;
		}
		node = details.data.node;
		path = details.data.path;
		access = details.data.access;
		await connect(fileId, node.name);
	}

	function retry() {
		if (!node) return;
		error = null;
		notice = null;
		reopens = 0;
		stop();
		void connect(node.id, node.name);
	}

	onMount(() => {
		void load(id);
		return () => {
			disposed = true;
			stop();
		};
	});
</script>

<svelte:head><title>{node?.name ?? 'Editor'} · Squirrel</title></svelte:head>

<Page title={node?.name ?? 'Editor'}>
	{#snippet above()}
		{#if node}
			{#if access === 'owner'}<Breadcrumbs {path} current={node.name} />
			{:else}<Breadcrumbs {path} current={node.name} rootLabel="Shared" rootHref="/shared" />{/if}
		{/if}
	{/snippet}
	{#snippet description()}
		<span class="status" role="status" data-testid="editor-status" data-phase={phase}>{status}</span>
		{#if others > 0}<span aria-hidden="true"> · </span>{others === 1 ? '1 other person' : `${others} other people`} here{/if}
	{/snippet}
	{#snippet actions()}
		{#if node && phase !== 'ended'}
			<div class="bar">
				{#if !canWrite && phase === 'connected'}<span data-testid="read-only"><Badge>Read only</Badge></span>{/if}
				{#if people.length > 0}
					<ul class="people" aria-label="People in this document">
						{#each people as person (person.key)}
							<li class="person" style:--person={person.color} title={person.self ? `${person.name} (you)` : person.name}>
								<span aria-hidden="true">{person.initials}</span>
								<span class="visually-hidden">{person.self ? `${person.name} (you)` : person.name}</span>
							</li>
						{/each}
					</ul>
				{/if}
			</div>
		{/if}
	{/snippet}

	{#if error}
		<Alert>{error}</Alert>
		{#if node}
			<div class="recover">
				<Button variant="secondary" onclick={retry}>Try again</Button>
				<Button variant="quiet" href={contentUrl(node.id, true)} download>Download</Button>
			</div>
		{/if}
	{:else if notice}
		<Alert tone="info">{notice}</Alert>
	{/if}
	<div class="editor" class:hidden={error !== null} bind:this={host} data-testid="editor"></div>
</Page>

<style>
	.status {
		font-variant-numeric: tabular-nums;
	}

	.bar {
		display: flex;
		align-items: center;
		justify-content: flex-end;
		gap: var(--space-3);
	}

	.people {
		display: flex;
		gap: var(--space-1);
		margin: 0;
		padding: 0;
		list-style: none;
	}

	.person {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		border-radius: var(--radius-full);
		background: var(--person);
		color: var(--card);
		font-size: var(--text-xs);
		line-height: var(--text-xs-lh);
		font-weight: var(--weight-strong);
		letter-spacing: var(--track-label);
		box-shadow: var(--shadow-knob);
	}

	.recover {
		display: flex;
		flex-wrap: wrap;
		gap: var(--space-2);
	}

	.editor {
		display: grid;
		min-height: var(--center-min);
		min-width: 0;
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
		overflow: hidden;
	}

	.editor:focus-within {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: var(--ring-offset);
	}

	.editor.hidden {
		display: none;
	}
</style>
