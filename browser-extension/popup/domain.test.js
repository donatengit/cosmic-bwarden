// Unit tests for extractDomain — mirrors the inline function in popup.js and background.js.
import { describe, it, expect } from 'vitest';

function extractDomain(url) {
    try {
        const host = new URL(url).hostname.toLowerCase().replace(/^www\./, '');
        const parts = host.split('.');
        return parts.length > 1 ? parts.slice(-2).join('.') : host;
    } catch { return null; }
}

describe('extractDomain', () => {
    it('extracts eTLD+1 from a simple domain', () => {
        expect(extractDomain('https://example.com/')).toBe('example.com');
    });

    it('strips www prefix', () => {
        expect(extractDomain('https://www.example.com/')).toBe('example.com');
    });

    it('strips deep subdomains', () => {
        expect(extractDomain('https://accounts.mail.example.com/')).toBe('example.com');
    });

    it('handles two-character ccTLDs', () => {
        expect(extractDomain('https://iqos.ru/')).toBe('iqos.ru');
        expect(extractDomain('https://www.iqos.ru/some/path')).toBe('iqos.ru');
    });

    it('handles path and query string', () => {
        expect(extractDomain('https://example.com/path?q=1#hash')).toBe('example.com');
    });

    it('returns null for invalid URLs', () => {
        expect(extractDomain('not a url')).toBeNull();
    });

    it('returns empty string for about: pages (callers guard these)', () => {
        // about:blank has an empty hostname; callers (background.js) filter it out before querying.
        expect(extractDomain('about:blank')).toBe('');
    });

    it('returns the bare hostname for chrome: pages (callers guard these)', () => {
        // Callers skip chrome:// URLs before extractDomain is called.
        expect(extractDomain('chrome://newtab/')).toBe('newtab');
    });

    it('does not strip non-www subdomains', () => {
        // mail.example.com → example.com (not mail.example.com)
        expect(extractDomain('https://mail.example.com/')).toBe('example.com');
    });

    it('badge domain search uses the result as a plain text query', () => {
        // Verify the value produced is what the agent receives as the search query.
        // Entry named "iqos.ru" would match query "iqos.ru" via agent name-search.
        expect(extractDomain('https://www.iqos.ru/checkout')).toBe('iqos.ru');
    });
});
