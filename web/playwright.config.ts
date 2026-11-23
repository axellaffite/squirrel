import { defineConfig } from '@playwright/test';

const port = process.env.SQUIRREL_E2E_PORT ?? '8099';
const baseURL = `http://127.0.0.1:${port}`;

// Row actions are inline from 768 px and behind an actions sheet below: viewports are explicit.
const desktop = { viewport: { width: 1280, height: 800 } };
const phone = { viewport: { width: 390, height: 844 }, isMobile: true, hasTouch: true };
// Photo dates are checked as an en-GB reader in Paris would see them.
const dates = { locale: 'en-GB', timezoneId: 'Europe/Paris' };

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
		{ name: 'files-phone', testMatch: 'files-phone.spec.ts', dependencies: ['onboarding'], use: phone },
		{ name: 'photos', testMatch: 'photos.spec.ts', dependencies: ['onboarding'], use: { ...desktop, ...dates } },
		{ name: 'photos-phone', testMatch: 'photos-phone.spec.ts', dependencies: ['onboarding'], use: { ...phone, ...dates } },
		{ name: 'documents', testMatch: 'documents.spec.ts', dependencies: ['onboarding'], use: { ...desktop, ...dates } },
		{ name: 'sharing', testMatch: 'sharing.spec.ts', dependencies: ['onboarding'], use: desktop },
		{ name: 'devices', testMatch: 'devices.spec.ts', dependencies: ['onboarding'], use: desktop },
		// Machine learning (skipped without models, failing on CI where they are installed).
		{ name: 'search', testMatch: 'search.spec.ts', dependencies: ['onboarding'], use: { ...desktop, ...dates } },
		{ name: 'people', testMatch: 'people.spec.ts', dependencies: ['onboarding'], use: { ...desktop, ...dates } },
		{ name: 'explore-phone', testMatch: 'explore-phone.spec.ts', dependencies: ['onboarding'], use: { ...phone, ...dates } },
		{ name: 'duplicates', testMatch: 'duplicates.spec.ts', dependencies: ['onboarding'], use: { ...desktop, ...dates } }
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
