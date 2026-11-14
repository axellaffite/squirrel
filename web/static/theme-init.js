// Applies the stored theme before first paint. A same-origin file, not an inline script, so the
// CSP (script-src 'self' + hashes) allows it. Mirrors src/lib/theme.ts (THEME_KEY).
try {
	var theme = localStorage.getItem('squirrel-theme');
	if (theme === 'light' || theme === 'dark') document.documentElement.dataset.theme = theme;
} catch (e) {
	// Storage blocked: follow the OS theme.
}
