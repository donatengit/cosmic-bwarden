// @vitest-environment jsdom
// Unit tests for the shared DOM primitives in content-dom.js: visibility
// (including ancestor and shadow chains), shadow-root traversal, form scoping
// and framework-safe value writes.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'content-dom.js'),
    'utf8'
);
const { isVisible, collectElements, formOf, treeRootOf, setInputValue } = new Function(
    `${source}\nreturn { isVisible, collectElements, formOf, treeRootOf, setInputValue };`
)();

function body(html) {
    document.body.innerHTML = html;
}

describe('isVisible', () => {
    it('accepts a plain visible input', () => {
        body('<input id="p" type="password">');
        expect(isVisible(document.getElementById('p'))).toBe(true);
    });

    it('rejects display:none on the element itself', () => {
        body('<input id="p" type="password" style="display: none">');
        expect(isVisible(document.getElementById('p'))).toBe(false);
    });

    // The case the element-only check missed: a descendant of a hidden wrapper
    // still computes display:block on itself.
    it('rejects an input inside a display:none wrapper', () => {
        body('<div style="display: none"><input id="p" type="password"></div>');
        expect(isVisible(document.getElementById('p'))).toBe(false);
    });

    it('rejects an input inside a visibility:hidden wrapper', () => {
        body('<div style="visibility: hidden"><input id="p" type="password"></div>');
        expect(isVisible(document.getElementById('p'))).toBe(false);
    });

    it('rejects an input inside a transparent wrapper', () => {
        body('<div style="opacity: 0.05"><input id="p" type="password"></div>');
        expect(isVisible(document.getElementById('p'))).toBe(false);
    });

    it('rejects a disabled input', () => {
        body('<input id="p" type="password" disabled>');
        expect(isVisible(document.getElementById('p'))).toBe(false);
    });

    // Anti-autofill sites keep the field readonly until focus; readonly blocks
    // typing, not script assignment, so the field is still a valid target.
    it('accepts a readonly input', () => {
        body('<input id="p" type="password" readonly>');
        expect(isVisible(document.getElementById('p'))).toBe(true);
    });

    it('walks through a shadow boundary to a hidden host', () => {
        body('<div id="host" style="display: none"></div>');
        const root = document.getElementById('host').attachShadow({ mode: 'open' });
        root.innerHTML = '<input id="p" type="password">';
        expect(isVisible(root.getElementById('p'))).toBe(false);
    });

    it('accepts a field inside a visible shadow root', () => {
        body('<div id="host"></div>');
        const root = document.getElementById('host').attachShadow({ mode: 'open' });
        root.innerHTML = '<input id="p" type="password">';
        expect(isVisible(root.getElementById('p'))).toBe(true);
    });
});

describe('collectElements', () => {
    it('finds matches in the document', () => {
        body('<form id="a"></form><form id="b"></form>');
        expect(collectElements(document, 'form').map(el => el.id)).toEqual(['a', 'b']);
    });

    it('descends into an open shadow root', () => {
        body('<div id="host"></div>');
        const root = document.getElementById('host').attachShadow({ mode: 'open' });
        root.innerHTML = '<input type="password" name="shadow-pass">';
        expect(collectElements(document, 'input[type="password"]')).toHaveLength(1);
    });

    it('descends through nested shadow roots', () => {
        body('<div id="outer"></div>');
        const outer = document.getElementById('outer').attachShadow({ mode: 'open' });
        outer.innerHTML = '<div id="inner"></div>';
        const inner = outer.getElementById('inner').attachShadow({ mode: 'open' });
        inner.innerHTML = '<input type="password" name="deep">';
        expect(collectElements(document, 'input[type="password"]')).toHaveLength(1);
    });

    it('includes the root itself when it matches', () => {
        body('<input id="p" type="password">');
        const input = document.getElementById('p');
        expect(collectElements(input, 'input[type="password"]')).toEqual([input]);
    });

    it('returns nothing for an empty root', () => {
        expect(collectElements(null, 'input')).toEqual([]);
    });
});

describe('formOf / treeRootOf', () => {
    it('returns the ancestor form', () => {
        body('<form id="f"><input id="p"></form>');
        expect(formOf(document.getElementById('p')).id).toBe('f');
    });

    it('returns a form associated through the form attribute', () => {
        body('<form id="f"></form><input id="p" form="f">');
        expect(formOf(document.getElementById('p')).id).toBe('f');
    });

    it('returns null for an input with no form', () => {
        body('<input id="p">');
        expect(formOf(document.getElementById('p'))).toBeNull();
    });

    it('scopes a formless shadow input to its shadow root', () => {
        body('<div id="host"></div>');
        const root = document.getElementById('host').attachShadow({ mode: 'open' });
        root.innerHTML = '<input id="p">';
        expect(treeRootOf(root.getElementById('p'))).toBe(root);
    });

    it('scopes a document input to the document', () => {
        body('<input id="p">');
        expect(treeRootOf(document.getElementById('p'))).toBe(document);
    });
});

describe('setInputValue', () => {
    it('sets the value and fires input + change', () => {
        body('<input id="p" type="password">');
        const input = document.getElementById('p');
        const events = [];
        input.addEventListener('input', () => events.push('input'));
        input.addEventListener('change', () => events.push('change'));

        setInputValue(input, 'secret1');

        expect(input.value).toBe('secret1');
        expect(events).toEqual(['input', 'change']);
    });

    // React installs a per-element `value` tracker: a plain `el.value = x`
    // updates the tracker's own snapshot too, so React sees no change and its
    // onChange never fires — the field looks filled, then the next render
    // sends the old value. The native prototype setter must bypass it.
    it('is seen as a change by a React-style value tracker', () => {
        body('<input id="p" type="password">');
        const input = document.getElementById('p');
        const proto = Object.getOwnPropertyDescriptor(window.HTMLInputElement.prototype, 'value');
        let tracked = '';
        Object.defineProperty(input, 'value', {
            configurable: true,
            get() { return proto.get.call(this); },
            set(v) { tracked = String(v); proto.set.call(this, v); },
        });

        let changes = 0;
        input.addEventListener('input', () => {
            if (proto.get.call(input) !== tracked) changes += 1;
            tracked = proto.get.call(input);
        });

        // What the old code did: invisible to the tracker.
        input.value = 'ignored';
        expect(changes).toBe(0);

        // What the extension does now.
        setInputValue(input, 'secret1');
        expect(changes).toBe(1);
        expect(proto.get.call(input)).toBe('secret1');
    });

    it('handles a textarea', () => {
        body('<textarea id="t"></textarea>');
        const el = document.getElementById('t');
        setInputValue(el, 'notes');
        expect(el.value).toBe('notes');
    });

    // A real user input event is composed, so a framework listening on a
    // container outside the field's shadow root still sees it. Without the
    // flag the value lands in the DOM and the app never learns about it.
    it('fires composed events out of a shadow root', () => {
        body('<div id="host"></div>');
        const root = document.getElementById('host').attachShadow({ mode: 'open' });
        root.innerHTML = '<input type="password">';
        const input = root.querySelector('input');

        const seen = [];
        root.addEventListener('input', (e) => seen.push(['input', e.composed]));
        root.addEventListener('change', (e) => seen.push(['change', e.composed]));

        setInputValue(input, 'secret1');

        expect(seen).toEqual([['input', true], ['change', true]]);
    });
});
