// @vitest-environment jsdom
// Unit tests for popup/icons.js. The regression these guard: the icon markup is
// parsed as XML, and an <svg> without an xmlns declaration is in no namespace —
// CSS still sizes the outer box, so layout and bounding-box assertions pass
// while nothing inside is ever painted (Firefox behaviour; jsdom is lenient).
import { describe, it, expect, beforeEach } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const here = path.dirname(fileURLToPath(import.meta.url));
const source = readFileSync(path.join(here, 'icons.js'), 'utf8');
const SVG_NS = 'http://www.w3.org/2000/svg';

let icons;

beforeEach(() => {
    document.body.innerHTML = '<button id="btn"></button>';
    // icons.js only defines constants and functions, so it evaluates cleanly;
    // return the pieces the tests inspect.
    icons = new Function(`${source}\nreturn { ICONS, ICON_NODES, setIcon };`)();
});

describe('ICONS markup', () => {
    it('declares the SVG namespace in every icon', () => {
        for (const [name, markup] of Object.entries(icons.ICONS)) {
            expect(markup, name).toContain(`xmlns="${SVG_NS}"`);
        }
    });

    it('parses every icon into the SVG namespace', () => {
        for (const [name, node] of icons.ICON_NODES) {
            if (node.namespaceURI === null) {
                // jsdom without the xmlns fix would report null here too, which
                // is the exact state that renders blank — never accept it.
                throw new Error(`${name} parsed into no namespace`);
            }
            expect(node.namespaceURI, name).toBe(SVG_NS);
        }
    });

    it('covers the three icons the popup builds dynamically', () => {
        expect(Object.keys(icons.ICONS).sort()).toEqual(['copy', 'eye', 'eye-off']);
    });
});

describe('setIcon', () => {
    it('replaces the button content with the named icon', () => {
        const btn = document.getElementById('btn');
        btn.textContent = 'stale';

        const returned = icons.setIcon(btn, 'eye');

        expect(returned).toBe(btn);
        expect(btn.querySelectorAll('svg')).toHaveLength(1);
        expect(btn.textContent).toBe('');
    });

    it('gives every call its own copy of the icon (no shared nodes)', () => {
        const a = icons.setIcon(document.getElementById('btn'), 'copy');
        const second = document.createElement('button');
        const b = icons.setIcon(second, 'copy');

        expect(a.firstChild).not.toBe(b.firstChild);
        a.firstChild.remove();
        expect(b.querySelectorAll('svg')).toHaveLength(1);
    });

    it('swaps the icon when called again on the same button', () => {
        const btn = document.getElementById('btn');
        const paths = () => [...btn.querySelectorAll('path')].map(p => p.getAttribute('d'));

        icons.setIcon(btn, 'eye');
        expect(paths()).not.toContain('M3.5 3.5l9 9');

        icons.setIcon(btn, 'eye-off');
        expect(paths()).toContain('M3.5 3.5l9 9');
    });

    it('clears the button for an unknown icon name instead of leaving a stale one', () => {
        const btn = document.getElementById('btn');
        icons.setIcon(btn, 'eye');
        icons.setIcon(btn, 'no-such-icon');

        expect(btn.querySelectorAll('svg')).toHaveLength(0);
        expect(btn.textContent).toBe('');
    });
});
