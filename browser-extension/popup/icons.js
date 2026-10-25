// Inline SVG icons for buttons built in JS (list rows, detail view, edit
// form). The static header icons live directly in popup.html; this module is
// the single source for everything created dynamically. All icons share one
// visual language: 16px grid, 1.5px strokes, currentColor — monochrome, with
// color coming from the button's own CSS (popup.css).
//
// Every string MUST carry xmlns="http://www.w3.org/2000/svg". They are parsed
// as XML (see ICON_NODES below), and an XML element with no namespace lands in
// no namespace at all: CSS still sizes the <svg> box, but nothing inside it is
// painted, so the button silently renders blank.
const ICONS = {
    eye:
        '<svg class="icon" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M1.75 8S4.25 4.25 8 4.25 14.25 8 14.25 8 11.75 11.75 8 11.75 1.75 8 1.75 8z"/>' +
        '<circle cx="8" cy="8" r="1.75"/>' +
        '</g></svg>',
    'eye-off':
        '<svg class="icon" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<path d="M1.75 8S4.25 4.25 8 4.25 14.25 8 14.25 8 11.75 11.75 8 11.75 1.75 8 1.75 8z"/>' +
        '<path d="M3.5 3.5l9 9"/>' +
        '</g></svg>',
    copy:
        '<svg class="icon" viewBox="0 0 16 16" xmlns="http://www.w3.org/2000/svg" aria-hidden="true" focusable="false">' +
        '<g fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">' +
        '<rect x="5.75" y="5.75" width="7.5" height="7.5" rx="1"/>' +
        '<path d="M10.25 5.75V4a1 1 0 0 0-1-1H4a1 1 0 0 0-1 1v5.25a1 1 0 0 0 1 1h1.75"/>' +
        '</g></svg>',
};

// Parse each markup string once, at load, into a detached SVG element; setIcon
// then clones it. Assigning the string to `el.innerHTML` would re-parse it for
// every list row and trips AMO's no-unsanitized scan (UNSAFE_VAR_ASSIGNMENT),
// which cannot see that ICONS holds only the constants above. Parsing as
// image/svg+xml is strict XML, so every value in ICONS must stay well-formed.
const ICON_NODES = new Map(
    Object.entries(ICONS).map(([name, markup]) => [
        name,
        new DOMParser().parseFromString(markup, 'image/svg+xml').documentElement,
    ])
);

// Replace a button's contents with the named icon (used with icon-only
// buttons; the button keeps its own title/aria-label).
function setIcon(el, name) {
    const icon = ICON_NODES.get(name);
    if (icon) {
        el.replaceChildren(icon.cloneNode(true));
    } else {
        // Unknown name: clear the button rather than leave a stale icon.
        el.replaceChildren();
    }
    return el;
}
