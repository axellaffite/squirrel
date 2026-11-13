import adapter from '@sveltejs/adapter-static';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

export default defineConfig({
	plugins: [
		sveltekit({
			compilerOptions: {
				// Force runes mode for the project, except for libraries. Can be removed in svelte 6.
				runes: ({ filename }) =>
					filename.split(/[/\\]/).includes('node_modules') ? undefined : true
			},
			// Static SPA: every unknown path falls back to index.html, which the Rust server embeds.
			adapter: adapter({ pages: 'build', assets: 'build', fallback: 'index.html', strict: false }),
			// Emits a <meta> CSP with hashes of SvelteKit's inline bootstrap script. The server adds
			// the directives a <meta> cannot carry (frame-ancestors) as response headers.
			csp: {
				mode: 'hash',
				directives: { 'script-src': ['self'], 'object-src': ['none'], 'base-uri': ['none'] }
			}
		})
	],
	server: { proxy: { '/api': 'http://localhost:8080' } },
	test: { include: ['src/**/*.test.ts'] }
});
