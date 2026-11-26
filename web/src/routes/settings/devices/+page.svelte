<script lang="ts">
	import { onMount } from 'svelte';
	import { api, errorMessage } from '#lib/api/client.ts';
	import { type Device, deviceLabel, lastSeen, sortDevices } from '#lib/devices.ts';
	import ConfirmDialog from '#lib/components/ConfirmDialog.svelte';
	import Icon from '#lib/components/icons/Icon.svelte';
	import Page from '#lib/components/shell/Page.svelte';
	import Alert from '#lib/components/ui/Alert.svelte';
	import Badge from '#lib/components/ui/Badge.svelte';
	import Button from '#lib/components/ui/Button.svelte';
	import Dialog from '#lib/components/ui/Dialog.svelte';
	import EmptyState from '#lib/components/ui/EmptyState.svelte';
	import IconButton from '#lib/components/ui/IconButton.svelte';
	import ListRow from '#lib/components/ui/ListRow.svelte';

	let devices: Device[] = $state([]);
	let loaded = $state(false);
	let error: string | null = $state(null);
	let now = $state(new Date());
	/** The device the confirmation dialog would revoke. */
	let revoking: Device | null = $state(null);
	/** The device whose actions sheet is open (phones). */
	let sheetFor: Device | null = $state(null);
	let sheetOpen = $state(false);
	let sheet: Dialog | undefined = $state();
	/** Set when the sheet's "Revoke" was chosen: the confirmation opens once the sheet closed. */
	let confirmAfterSheet: Device | null = null;

	async function refresh(): Promise<void> {
		try {
			const { data, error: apiError, response } = await api.GET('/api/devices');
			if (!data) {
				error = errorMessage(apiError, response);
				return;
			}
			devices = sortDevices(data);
			now = new Date();
			loaded = true;
			error = null;
		} catch {
			error = errorMessage(null);
		}
	}

	async function revoke(device: Device): Promise<string | null> {
		try {
			const { error: apiError, response } = await api.DELETE('/api/devices/{id}', {
				params: { path: { id: device.id } }
			});
			if (!response.ok) return errorMessage(apiError, response);
		} catch {
			return errorMessage(null);
		}
		await refresh();
		return null;
	}

	function chooseRevoke(device: Device) {
		confirmAfterSheet = device;
		sheet?.close();
	}

	function sheetClosed() {
		sheetFor = null;
		revoking = confirmAfterSheet;
		confirmAfterSheet = null;
	}

	onMount(refresh);
</script>

<svelte:head><title>Devices · Squirrel</title></svelte:head>

<Page title="Devices" back={{ href: '/settings', label: 'Settings' }}>
	{#snippet description()}Phones signed in to your Squirrel. Revoking one signs it out at once; what it backed up
		stays.{/snippet}

	{#if error}<Alert>{error}</Alert>{/if}

	{#if devices.length > 0}
		<div class="well">
			<ul class="rows" aria-label="Devices">
				{#each devices as device, index (device.id)}
					<ListRow selected={revoking?.id === device.id} arrive={index} testId="device-row">
						{#snippet icon()}<Icon name="smartphone" />{/snippet}
						{#snippet title()}{device.name}{/snippet}
						{#snippet meta()}
							{deviceLabel(device)}<span class="sep" aria-hidden="true">·</span>{lastSeen(device, now)}
						{/snippet}
						{#snippet actions()}
							{#if device.revoked_at}
								<Badge tone="danger">Revoked</Badge>
							{:else}
								<span class="inline">
									<Button
										variant="danger"
										size="sm"
										aria-label={`Revoke ${device.name}`}
										onclick={() => (revoking = device)}>Revoke</Button
									>
								</span>
								<span class="more">
									<IconButton
										icon="more"
										label={`Actions for ${device.name}`}
										tooltip="Actions"
										onclick={() => {
											sheetFor = device;
											sheetOpen = true;
										}}
									/>
								</span>
							{/if}
						{/snippet}
					</ListRow>
				{/each}
			</ul>
		</div>
	{:else if loaded && !error}
		<EmptyState icon="smartphone" title="No phones yet">Install the Squirrel app and sign in to back up your photos.</EmptyState>
	{/if}
</Page>

{#if sheetFor}
	<Dialog bind:this={sheet} bind:open={sheetOpen} title={sheetFor.name} onclose={sheetClosed}>
		<ul class="sheet-actions">
			<li>
				<button
					type="button"
					class="sheet-action"
					aria-label={`Revoke ${sheetFor.name}`}
					onclick={() => sheetFor && chooseRevoke(sheetFor)}
				>
					<span class="sheet-well"><Icon name="log-out" /></span>Revoke
				</button>
			</li>
		</ul>
	</Dialog>
{/if}

{#if revoking}
	{@const device = revoking}
	<ConfirmDialog
		title={`Revoke ${device.name}?`}
		confirmLabel="Revoke"
		onConfirm={() => revoke(device)}
		onClose={() => (revoking = null)}
	>
		{device.name} is signed out at once and stops backing up. What it already backed up stays in your
		Squirrel. Sign in on the phone again to resume.
	</ConfirmDialog>
{/if}

<style>
	.well {
		padding: var(--well-pad);
		border-radius: var(--radius-well);
		background: var(--tint);
	}

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

	/* Inline from 768 px; one "Actions" button opening a sheet on phones, like file rows. */
	.inline {
		display: none;
	}

	.more {
		display: contents;
	}

	@media (min-width: 48rem) {
		.inline {
			display: contents;
		}

		.more {
			display: none;
		}
	}

	.sheet-actions {
		display: grid;
		gap: var(--space-1);
		margin: 0;
		padding: var(--space-2);
		list-style: none;
		border-radius: var(--radius-well);
		background: var(--tint);
	}

	.sheet-action {
		display: flex;
		align-items: center;
		gap: var(--space-3);
		width: 100%;
		min-height: var(--row-min);
		padding: var(--space-2);
		border: 0;
		border-radius: var(--radius-card);
		background: transparent;
		color: var(--danger);
		font-size: var(--text-md);
		font-weight: var(--weight-label);
		text-align: left;
		cursor: pointer;
		transition:
			background-color var(--dur-fast) var(--ease),
			box-shadow var(--dur) var(--ease),
			transform var(--dur-fast) var(--ease);
	}

	.sheet-well {
		display: grid;
		place-items: center;
		width: var(--well-sm);
		height: var(--well-sm);
		flex: none;
		border-radius: var(--radius-full);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	@media (hover: hover) {
		.sheet-action:hover {
			background: var(--card);
			box-shadow: var(--shadow-card);
		}
	}

	.sheet-action:focus-visible {
		outline: var(--ring-width) solid var(--accent);
		outline-offset: calc(-1 * var(--ring-width));
	}

	.sheet-action:active {
		transform: translateY(var(--press));
	}
</style>
