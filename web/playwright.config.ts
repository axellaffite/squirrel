import { defineConfig } from '@playwright/test';

// Keep in sync with scripts/e2e-server.sh, which reads the same variable.
const port = process.env.SQUIRREL_E2E_PORT ?? '8099';
const baseURL = `http://127.0.0.1:${port}`;

export default defineConfig({
	testDir: 'e2e',
	workers: 1,
	forbidOnly: !!process.env.CI,
	reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
	use: { baseURL, trace: 'retain-on-failure' },
	webServer: {
		command: 'bash ../scripts/e2e-server.sh',
		url: `${baseURL}/api/healthz`,
		timeout: 600_000,
		reuseExistingServer: false,
		stdout: 'pipe',
		stderr: 'pipe'
	}
});
