// @vitest-environment jsdom
// Unit tests for the pure grouping/heuristic logic in content-generate.js —
// which password fields get the inline "generate" icon, and which don't.
import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'content-generate.js'),
    'utf8'
);

// The script's top-level code registers a browser.runtime.onMessage listener
// and does an initial (harmless, empty-document) scanAndDecorate(document) —
// stub `browser` so that doesn't throw, mirroring the Playwright injection stub.
globalThis.browser = { runtime: { onMessage: { addListener: () => {} } } };

// Vitest files are ESM (strict mode), where eval() keeps function
// declarations local — load through new Function and return the helpers.
const { passwordGroupsIn } = new Function(
    `${source}\nreturn { passwordGroupsIn };`
)();

function setBody(html) {
    document.body.innerHTML = html;
    return document.body;
}

function names(group) {
    return group.map((el) => el.name).sort();
}

describe('passwordGroupsIn', () => {
    it('does not decorate a plain login form (one password field, no signal)', () => {
        const root = setBody(`
            <form id="login-form">
                <input type="text" name="username">
                <input type="password" name="password">
            </form>
        `);
        expect(passwordGroupsIn(root)).toEqual([]);
    });

    it('decorates a registration form (one password field, id/name signals "register")', () => {
        const root = setBody(`
            <form id="register-form">
                <input type="email" name="email">
                <input type="password" name="reg-password">
            </form>
        `);
        const groups = passwordGroupsIn(root);
        expect(groups).toHaveLength(1);
        expect(names(groups[0])).toEqual(['reg-password']);
    });

    it('decorates both fields of an unmarked two-password confirm form', () => {
        const root = setBody(`
            <form id="signup-thing">
                <input type="password" name="new">
                <input type="password" name="confirm">
            </form>
        `);
        const groups = passwordGroupsIn(root);
        expect(groups).toHaveLength(1);
        expect(names(groups[0])).toEqual(['confirm', 'new']);
    });

    it('excludes autocomplete=current-password from a change-password form', () => {
        const root = setBody(`
            <form id="change-password-form">
                <input type="password" name="current-password" autocomplete="current-password">
                <input type="password" name="new-password" autocomplete="new-password">
                <input type="password" name="confirm-password" autocomplete="new-password">
            </form>
        `);
        const groups = passwordGroupsIn(root);
        expect(groups).toHaveLength(1);
        expect(names(groups[0])).toEqual(['confirm-password', 'new-password']);
    });

    it('does not decorate a lone current-password field even in a named scope', () => {
        // Pathological: a form literally named "register" but whose only
        // password field is explicitly the current one — never happens in
        // practice, but the current-password exclusion must win regardless.
        const root = setBody(`
            <form id="register-form">
                <input type="password" name="pw" autocomplete="current-password">
            </form>
        `);
        expect(passwordGroupsIn(root)).toEqual([]);
    });

    it('finds password fields outside any form', () => {
        const root = setBody(`
            <input type="email" name="email">
            <input type="password" name="reg-password" autocomplete="new-password">
        `);
        const groups = passwordGroupsIn(root);
        expect(groups).toHaveLength(1);
        expect(names(groups[0])).toEqual(['reg-password']);
    });

    it('does not double-count a form password field when scanning the document root', () => {
        const root = setBody(`
            <form id="register-form">
                <input type="password" name="reg-password" autocomplete="new-password">
            </form>
        `);
        const groups = passwordGroupsIn(root);
        expect(groups).toHaveLength(1);
        expect(groups[0]).toHaveLength(1);
    });
});
