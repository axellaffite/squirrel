import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Keep in sync with scripts/e2e-server.sh.
const DATABASE_URL =
	process.env.SQUIRREL_E2E_DATABASE_URL ?? 'postgres://squirrel:squirrel@localhost:55432/squirrel_e2e';
const TARGET_DIR = process.env.CARGO_TARGET_DIR ?? fileURLToPath(new URL('../../../target', import.meta.url));

/** Sets a user's quota with `squirrel admin set-quota` (bytes, or 'unlimited'). */
export function setQuota(username: string, quota: number | 'unlimited'): void {
	execFileSync(`${TARGET_DIR}/debug/squirrel`, ['admin', 'set-quota', username, String(quota)], {
		env: { ...process.env, SQUIRREL_DATABASE_URL: DATABASE_URL },
		stdio: 'pipe'
	});
}
