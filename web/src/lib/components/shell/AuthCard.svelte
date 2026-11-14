<script lang="ts">
	import type { Snippet } from 'svelte';

	interface Props {
		title: string;
		subtitle?: string;
		children: Snippet;
		/** Small print under the card body. */
		footer?: Snippet;
	}

	let { title, subtitle, children, footer }: Props = $props();
</script>

<main class="auth">
	<p class="wordmark arrive" style:--i={0}>Squirrel</p>
	<section class="card arrive" style:--i={1} aria-labelledby="auth-title">
		<header>
			<h1 id="auth-title">{title}</h1>
			{#if subtitle}<p class="muted">{subtitle}</p>{/if}
		</header>
		{@render children()}
		{#if footer}<footer class="muted">{@render footer()}</footer>{/if}
	</section>
</main>

<style>
	/* The grey ground holds the wordmark and one white card. */
	.auth {
		display: grid;
		justify-items: center;
		align-content: center;
		gap: var(--space-5);
		min-height: 100svh;
		padding: calc(var(--space-6) + env(safe-area-inset-top)) var(--gutter)
			calc(var(--space-6) + env(safe-area-inset-bottom));
	}

	.wordmark {
		font-size: var(--text-2xl);
		line-height: var(--text-2xl-lh);
		font-weight: var(--weight-strong);
		letter-spacing: var(--track-tight);
		color: var(--ink);
	}

	.card {
		display: grid;
		gap: var(--space-5);
		width: 100%;
		max-width: var(--auth-max);
		padding: var(--space-5);
		border-radius: var(--radius-panel);
		background: var(--card);
		box-shadow: var(--shadow-card);
	}

	@media (min-width: 48rem) {
		.card {
			padding: var(--space-6);
		}
	}

	header {
		display: grid;
		gap: var(--space-1);
	}

	header p {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	footer {
		font-size: var(--text-sm);
		line-height: var(--text-sm-lh);
	}

	@media (forced-colors: active) {
		.card {
			border: 1px solid CanvasText;
		}
	}
</style>
