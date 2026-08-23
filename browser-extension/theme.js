// Shared theme tokens for the in-page (shadow-DOM) surfaces: the save bar
// (content-bar.js) and the generate icon (content-generate.js). Shadow roots
// cannot see the popup's stylesheet, so these scripts build their own
// <style> — and this object is the single source they interpolate.
// Values MUST stay in sync with popup/popup.css (:root and the dark
// prefers-color-scheme block) — both describe the same palette.
const THEME = {
    light: {
        'bg': '#ffffff',
        'surface': '#f7f7f8',
        'surface-hover': '#f0f0f1',
        'border': '#e6e6e8',
        'text': '#1a1a1a',
        'text-secondary': '#5c5f66',
        'text-tertiary': '#6f727a',
        'accent': '#175DDC',
        'accent-hover': '#0f4bb8',
        'accent-tint': '#eaf1fe',
        'on-accent': '#ffffff',
        'danger': '#d92d20',
        'radius': '12px',
        'radius-sm': '8px',
        'shadow-sm': '0 1px 2px rgba(16, 24, 40, 0.08)',
        'shadow': '0 4px 12px rgba(16, 24, 40, 0.12)',
    },
    dark: {
        'bg': '#171717',
        'surface': '#222326',
        'surface-hover': '#2b2c30',
        'border': '#3a3a3d',
        'text': '#f2f2f3',
        'text-secondary': '#a6a8ad',
        'text-tertiary': '#8f929a',
        'accent': '#7da6f5',
        'accent-hover': '#a3c1f8',
        'accent-tint': '#1c2c4f',
        'on-accent': '#0f172a',
        'danger': '#f97066',
        'radius': '12px',
        'radius-sm': '8px',
        'shadow-sm': '0 1px 2px rgba(0, 0, 0, 0.4)',
        'shadow': '0 4px 12px rgba(0, 0, 0, 0.45)',
    },
};

// The token set matching the page's current color scheme, evaluated at call
// time so a theme switch is picked up by the next surface created.
function themeColors() {
    const dark = typeof matchMedia === 'function'
        && matchMedia('(prefers-color-scheme: dark)').matches;
    return dark ? THEME.dark : THEME.light;
}

// CSS custom-property declarations for a shadow host. Applied as inline style
// on the host element, e.g. `host.style.cssText = 'display:block;' + themeCss();`
// — the shadow <style> then uses var(--accent), var(--radius), ... exactly
// like popup.css does.
function themeCss() {
    const t = themeColors();
    return Object.entries(t).map(([k, v]) => `--${k}:${v};`).join('');
}
