/**
 * Theme choice: follow the OS ("system") or force light/dark through data-theme on <html>.
 * static/theme-init.js applies the stored choice before first paint; keep THEME_KEY in sync.
 */

export type ThemeChoice = 'system' | 'light' | 'dark';

export const THEME_KEY = 'squirrel-theme';
export const THEME_CHOICES: readonly ThemeChoice[] = ['system', 'light', 'dark'];

type ThemeStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

interface ThemeRoot {
	dataset: Record<string, string | undefined>;
}

export function parseTheme(value: string | null | undefined): ThemeChoice {
	return value === 'light' || value === 'dark' ? value : 'system';
}

/** Storage can be missing or throw (private mode, blocked site data): fall back to system. */
export function readTheme(storage: ThemeStorage | null = safeLocalStorage()): ThemeChoice {
	try {
		return parseTheme(storage?.getItem(THEME_KEY));
	} catch {
		return 'system';
	}
}

export function saveTheme(choice: ThemeChoice, storage: ThemeStorage | null = safeLocalStorage()): void {
	try {
		if (choice === 'system') storage?.removeItem(THEME_KEY);
		else storage?.setItem(THEME_KEY, choice);
	} catch {
		// Not remembered this time; the choice still applies to the current page.
	}
}

export function applyTheme(choice: ThemeChoice, root: ThemeRoot = document.documentElement): void {
	if (choice === 'system') delete root.dataset.theme;
	else root.dataset.theme = choice;
}

function safeLocalStorage(): ThemeStorage | null {
	try {
		return typeof localStorage === 'undefined' ? null : localStorage;
	} catch {
		return null;
	}
}
