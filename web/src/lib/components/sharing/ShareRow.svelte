<script lang="ts">
	import Icon from '#lib/components/icons/Icon.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';
	import SegmentedControl from '#lib/components/ui/SegmentedControl.svelte';
	import type { Permission, Share } from '#lib/sharing/api.ts';

	interface Props {
		share: Share;
		onChange: (permission: Permission) => void;
		onStop: () => void;
	}

	let { share, onChange, onStop }: Props = $props();

	const PERMISSIONS = [
		{ value: 'read', label: 'Read' },
		{ value: 'write', label: 'Write' }
	] as const;
	const name = $derived(share.person.display_name);
</script>

<ListRow testId="share-row">
	{#snippet icon()}<Icon name="user" />{/snippet}
	{#snippet title()}{name}{/snippet}
	{#snippet meta()}@{share.person.username}{/snippet}
	{#snippet actions()}
		<SegmentedControl
			label={`Permission for ${name}`}
			name={`share-${share.id}`}
			options={PERMISSIONS}
			value={share.permission}
			onchange={onChange}
		/>
		<IconButton icon="x" label={`Stop sharing with ${name}`} tooltip="Stop sharing" onclick={onStop} />
	{/snippet}
</ListRow>
