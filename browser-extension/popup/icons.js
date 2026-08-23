// Inline SVG icons for buttons built in JS (list rows, detail view, edit
// form). The static header icons live directly in popup.html; this module is
// the single source for everything created dynamically. All icons share one
// visual language: 16px grid, 1.5px strokes, currentColor — monochrome, with
// color coming from the button's own CSS (popup.css).
const ICONS = {
    eye:
        '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M1.75 8S4.25 4.25 8 4.25 14.25 8 14.25 8 11.75 11.75 8 11.75 1.75 8 1.75 8z"/>' +
        '<circle cx="8" cy="8" r="1.75"/>' +
        '</g></svg>',
    'eye-off':
        '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M1.75 8S4.25 4.25 8 4.25 14.25 8 14.25 8 11.75 11.75 8 11.75 1.75 8 1.75 8z"/>' +
        '<path d="M3.5 3.5l9 9"/>' +
        '</g></svg>',
    copy:
        '<svg class="icon" viewBox="0 0 16 16" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<rect x="5.75" y="5.75" width="7.5" height="7.5" rx="1"/>' +
        '<path d="M10.25 5.75V4a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v5.25a1 1 0 0 0 1 1h1.75"/>' +
        '</g></svg>',
};

// Replace a button's contents with the named icon (used with icon-only
// buttons; the button keeps its own title/aria-label).
function setIcon(el, name) {
    el.innerHTML = ICONS[name] || '';
    return el;
}
