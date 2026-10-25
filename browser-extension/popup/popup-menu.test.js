// @vitest-environment jsdom
// Unit tests for popup-menu.js: the dropdown plumbing and WAI-ARIA
// menu-button keyboard model shared by the per-row copy menu and the header
// generator menu. Kept apart from popup-generator.test.js so each file covers
// one module.
import { describe, it, expect, beforeEach, afterEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const here = path.dirname(fileURLToPath(import.meta.url));
const menuSrc = readFileSync(path.join(here, 'popup-menu.js'), 'utf8');
const listActionsSrc = readFileSync(path.join(here, 'popup-list-actions.js'), 'utf8');

// Concatenating the real sources keeps popup-menu.js's `openDropdown` binding
// shared with popup-list-actions.js exactly as the <script> tags share it.
function loadHarness() {
    const source = `
        function setIcon(el) { return el; }
        function showStatus() {}
        ${menuSrc}
        ${listActionsSrc}
        return {
            showDropdownMenu,
            makeDropdownItem,
            makeCopyDropdownBtn,
            closeOpenDropdown,
            getOpenMenu: () => openDropdown,
        };
    `;
    return new Function(source)();
}

function press(el, key) {
    el.dispatchEvent(new window.KeyboardEvent('keydown', { key, bubbles: true }));
}

let harness;
let trigger;
let onSelectCalls;

function buildMenu() {
    onSelectCalls = [];
    const menu = document.createElement('div');
    menu.className = 'copy-dropdown-menu';
    menu.append(
        harness.makeDropdownItem('First', async () => { onSelectCalls.push('first'); }),
        harness.makeDropdownItem('Second', async () => { onSelectCalls.push('second'); }),
    );
    trigger = document.getElementById('trigger');
    harness.showDropdownMenu(menu, trigger);
    return menu;
}

const menuEl = () => document.querySelector('.copy-dropdown-menu');
const items = () => [...document.querySelectorAll('.copy-dropdown-menu [role="menuitem"]')];

beforeEach(() => {
    document.body.innerHTML = '<button id="trigger" aria-haspopup="menu" aria-expanded="false">menu</button>';
    harness = loadHarness();
});

afterEach(() => {
    harness.closeOpenDropdown();
});

describe('showDropdownMenu', () => {
    it('marks the menu and its items, sets aria-expanded and focuses the first item', () => {
        const menu = buildMenu();

        expect(menu.getAttribute('role')).toBe('menu');
        expect(trigger.getAttribute('aria-expanded')).toBe('true');
        expect(items().map(i => i.tabIndex)).toEqual([-1, -1]);
        expect(document.activeElement).toBe(items()[0]);
    });

    it('replaces an already-open menu instead of stacking a second one', () => {
        buildMenu();
        const second = buildMenu();

        expect(document.querySelectorAll('.copy-dropdown-menu')).toHaveLength(1);
        expect(menuEl()).toBe(second);
    });
});

describe('keyboard model', () => {
    it('moves focus with ArrowDown/ArrowUp and wraps at both ends', () => {
        buildMenu();

        press(items()[0], 'ArrowDown');
        expect(document.activeElement).toBe(items()[1]);
        press(items()[1], 'ArrowDown');
        expect(document.activeElement).toBe(items()[0]);
        press(items()[0], 'ArrowUp');
        expect(document.activeElement).toBe(items()[1]);
    });

    it('jumps to the first and last item with Home and End', () => {
        buildMenu();

        press(items()[1], 'Home');
        expect(document.activeElement).toBe(items()[0]);
        press(items()[0], 'End');
        expect(document.activeElement).toBe(items()[1]);
    });

    it('Escape closes and returns focus to the trigger', () => {
        buildMenu();

        press(items()[0], 'Escape');

        expect(menuEl()).toBeNull();
        expect(document.activeElement).toBe(trigger);
        expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });

    it('Tab closes and hands focus back to the trigger', () => {
        buildMenu();

        press(items()[0], 'Tab');

        expect(menuEl()).toBeNull();
        expect(document.activeElement).toBe(trigger);
    });

    it('Escape outside the menu also closes it', () => {
        buildMenu();
        document.body.focus();

        document.dispatchEvent(new window.KeyboardEvent('keydown', { key: 'Escape' }));

        expect(menuEl()).toBeNull();
        expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });

    it('arrow keys do nothing when the menu has no items', () => {
        const menu = document.createElement('div');
        menu.className = 'copy-dropdown-menu';
        harness.showDropdownMenu(menu, trigger);

        press(menu, 'ArrowDown');

        expect(document.querySelector('[role="menuitem"]')).toBeNull();
        expect(harness.getOpenMenu()).toBe(menu);
    });
});

describe('item activation and dismissal', () => {
    it('runs the selected item, closes the menu and restores focus', async () => {
        buildMenu();

        items()[1].click();
        await new Promise(resolve => setTimeout(resolve, 0));

        expect(onSelectCalls).toEqual(['second']);
        expect(menuEl()).toBeNull();
        expect(document.activeElement).toBe(trigger);
    });

    it('closes on an outside click', () => {
        buildMenu();

        document.body.click();

        expect(menuEl()).toBeNull();
        expect(trigger.getAttribute('aria-expanded')).toBe('false');
    });
});

describe('per-row copy dropdown', () => {
    it('uses the same semantics and label', () => {
        const wrap = harness.makeCopyDropdownBtn({ id: '1', username: 'ada' });
        document.body.appendChild(wrap);
        const toggle = wrap.querySelector('button');

        expect(toggle.getAttribute('aria-haspopup')).toBe('menu');
        expect(toggle.getAttribute('aria-expanded')).toBe('false');

        toggle.click();

        const menu = document.querySelector('.copy-dropdown-menu');
        expect(menu.getAttribute('role')).toBe('menu');
        expect(menu.getAttribute('aria-label')).toBe('Copy username or password');
        expect(toggle.getAttribute('aria-expanded')).toBe('true');
        expect(document.activeElement).toBe(menu.querySelector('[role="menuitem"]'));

        press(document.activeElement, 'Escape');
        expect(document.querySelector('.copy-dropdown-menu')).toBeNull();
        expect(toggle.getAttribute('aria-expanded')).toBe('false');
        expect(document.activeElement).toBe(toggle);
    });
});
