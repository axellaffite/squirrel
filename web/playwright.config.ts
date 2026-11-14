import { defineConfig } from '@playwright/test';

const port = process.env.SQUIRREL_E2E_PORT ?? '8099';
const baseURL = `http://127.0.0.1:${port}`;

// Row actions are inline from 768 px and behind an actions sheet below: viewports are explicit.
const desktop = { viewport: { width: 1280, height: 800 } };
const phone = { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true };

export default defineConfig({
	testDir: 'e2e',
	workers: 1,
	forbidOnly: !!process.env.CI,
	reporter: process.env.CI ? [['list'], ['html', { open: 'never' }]] : 'list',
	use: { baseURL, trace: 'retain-on-failure' },
	// The file specs log in with the accounts onboarding.spec.ts creates.
	projects: [
		{ name: 'onboarding', testMatch: 'onboarding.spec.ts', use: desktop },
		{ name: 'files', testMatch: 'files.spec.ts', dependencies: ['onboarding'], use: desktop },
		{ name: 'files-phone', testMatch: 'files-phone.spec.ts', dependencies: ['onboarding'], use: phone }
	],
	webServer: {
		command: 'bash ../scripts/e2e-server.sh',
		url: `${baseURL}/api/healthz`,
		timeout: 600_000,
		reuseExistingServer: false,
		stdout: 'pipe',
		stderr: 'pipe'
	}
});
