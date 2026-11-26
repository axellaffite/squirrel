<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import type { IconName } from '#lib/components/icons/paths.ts';
	import Page from '#lib/components/shell/Page.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import { session } from '#lib/session.svelte.ts';

	interface Entry {
		href: string;
		label: string;
		line: string;
		icon: IconName;
	}

	const entries: Entry[] = $derived([
		{ href: '/settings/appearance', label: 'Appearance', line: 'The sidebar image on this device', icon: 'image' },
		{ href: '/settings/friends', label: 'Friends', line: 'People you share with', icon: 'users' },
		{ href: '/settings/devices', label: 'Devices', line: 'Phones signed in to your Squirrel', icon: 'smartphone' },
		{ href: '/settings/faces', label: 'Faces', line: 'Recognising people in your photos', icon: 'scan-face' },
		...(session.me?.can_invite
			? [{ href: '/settings/invitations', label: 'Invitations', line: 'Invite someone to this Squirrel', icon: 'user-plus' as const }]
			: [])
	]);
</script>

<svelte:head><title>Settings · Squirrel</title></svelte:head>

<Page title="Settings">
	{#snippet description()}Your account, your devices and how Squirrel looks.{/snippet}
	<ul class="rows" aria-label="Settings">
		{#each entries as entry, index (entry.href)}
			<ListRow arrive={index + 1}>
				{#snippet icon()}<Icon name={entry.icon} />{/snippet}
				{#snippet title()}<a class="row-link" href={entry.href}>{entry.label}</a>{/snippet}
				{#snippet meta()}{entry.line}{/snippet}
				{#snippet actions()}<span class="chevron" aria-hidden="true"><Icon name="chevron-right" /></span>{/snippet}
			</ListRow>
		{/each}
	</ul>
</Page>

<style>
	/* One grouped card; ListRow draws the hairline between rows (MASTER.md › "Table"). */
	.rows {
		display: grid;
		margin: 0;
		padding: 0;
		list-style: none;
		border: 1px solid var(--hair);
		border-radius: var(--radius-card);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	.chevron {
		display: grid;
		place-items: center;
		padding-inline: var(--space-2);
		color: var(--ink-muted);
	}
</style>
