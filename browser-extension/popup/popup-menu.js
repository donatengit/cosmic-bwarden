// Shared dropdown/menu plumbing for the popup: the one open menu, its
// positioning, and the WAI-ARIA menu-button keyboard model. Two surfaces use
// it — the per-row copy dropdown (popup-list-actions.js) and the header
// password-generator menu (popup-generator.js).
//
// Loaded before both of them (see popup.html), and before popup.js: the
// document-level listeners below are the only work at this file's top level.
// Every other reference (document, window) is a browser global, so load order
// matters only in that this file must run before the first menu is opened.

let openDropdown = null;

function menuItems(menu) {
    return [...menu.querySelectorAll('[role="menuitem"]')];
}

function focusMenuItem(menu, index) {
    const items = menuItems(menu);
    if (items.length === 0) return;
    items[(index + items.length) % items.length].focus();
}

// Close the open menu. `restoreFocus` returns focus to its trigger, which is
// where a keyboard user expects to be once the menu is gone (Escape, Tab, or
// picking an item). Mouse users see no difference: a programmatic focus after
// a pointer click does not draw a focus ring.
function closeOpenDropdown({ restoreFocus = false } = {}) {
    if (!openDropdown) return;
    const anchor = openDropdown.anchorBtn;
    if (anchor) anchor.setAttribute('aria-expanded', 'false');
    openDropdown.remove();
    openDropdown = null;
    if (restoreFocus && anchor) anchor.focus();
}

document.addEventListener('click', () => closeOpenDropdown());
document.addEventListener('keydown', e => {
    if (e.key === 'Escape') closeOpenDropdown({ restoreFocus: true });
});

// #results scrolls (overflow-y: auto) and tightly wraps its rows, so a menu
// nested inside a row would get clipped by that ancestor whenever it
// overflows the row's box — which happens with any list short enough not to
// need scrolling. Appending to <body> with position:fixed, positioned from
// the button's viewport rect, sidesteps that clipping entirely.
function positionDropdown(menu, anchorBtn) {
    const btnRect = anchorBtn.getBoundingClientRect();
    const menuHeight = menu.offsetHeight;
    const menuWidth = menu.offsetWidth;
    const openUpward = btnRect.bottom + menuHeight > window.innerHeight;
    menu.style.top = `${openUpward ? btnRect.top - menuHeight : btnRect.bottom}px`;
    menu.style.left = `${Math.min(btnRect.right - menuWidth, window.innerWidth - menuWidth - 4)}px`;
}

// WAI-ARIA menu-button keyboard model: arrows and Home/End move between items,
// Escape and Tab close and return to the trigger, Enter/Space activate a
// button natively. Items are marked role=menuitem with tabindex -1 and are
// reached by arrow keys only, so Tab is always "leave the menu".
function onMenuKeydown(e) {
    const menu = e.currentTarget;
    const items = menuItems(menu);
    const index = items.indexOf(document.activeElement);
    switch (e.key) {
        case 'ArrowDown':
            e.preventDefault();
            focusMenuItem(menu, index + 1);
            break;
        case 'ArrowUp':
            e.preventDefault();
            focusMenuItem(menu, index - 1);
            break;
        case 'Home':
            e.preventDefault();
            focusMenuItem(menu, 0);
            break;
        case 'End':
            e.preventDefault();
            focusMenuItem(menu, items.length - 1);
            break;
        case 'Escape':
            e.preventDefault();
            // Stop propagation so the document-level handler does not close
            // the menu a second time.
            e.stopPropagation();
            closeOpenDropdown({ restoreFocus: true });
            break;
        case 'Tab':
            // Close and hand focus to the trigger before the browser resolves
            // the next target, so Tab continues from the button.
            closeOpenDropdown({ restoreFocus: true });
            break;
        default:
            break;
    }
}

// Anchor `menu` (already populated with makeDropdownItem buttons) under
// `anchorBtn`, focus its first item and enable the keyboard model. The trigger
// must carry aria-haspopup="menu"; aria-expanded is maintained here.
function showDropdownMenu(menu, anchorBtn) {
    closeOpenDropdown();
    menu.setAttribute('role', 'menu');
    document.body.appendChild(menu);
    positionDropdown(menu, anchorBtn);
    menu.anchorBtn = anchorBtn;
    menu.addEventListener('keydown', onMenuKeydown);
    openDropdown = menu;
    anchorBtn.setAttribute('aria-expanded', 'true');
    focusMenuItem(menu, 0);
}

function makeDropdownItem(label, onSelect) {
    const btn = document.createElement('button');
    btn.type = 'button';
    btn.setAttribute('role', 'menuitem');
    btn.tabIndex = -1;
    btn.textContent = label;
    btn.onclick = async e => {
        e.stopPropagation();
        closeOpenDropdown({ restoreFocus: true });
        await onSelect();
    };
    return btn;
}
