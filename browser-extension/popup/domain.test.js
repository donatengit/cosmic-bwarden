// Unit tests for extractDomain — mirrors the inline function in popup.js and background.js.
import { describe, it, expect } from 'vitest';

function extractDomain(url) {
    try {
        // Full registrable host, only stripping a leading "www." — NOT the last
        // two labels (which mishandles multi-part public suffixes like ".co.uk").
        return new URL(url).hostname.toLowerCase().replace(/^www\./, '');
    } catch { return null; }
}

describe('extractDomain', () => {
    it('returns the host for a simple domain', () => {
        expect(extractDomain('https://example.com/')).toBe('example.com');
    });

    it('strips a leading www prefix', () => {
        expect(extractDomain('https://www.example.com/')).toBe('example.com');
    });

    it('keeps subdomains (does not over-collapse)', () => {
        expect(extractDomain('https://accounts.mail.example.com/')).toBe('accounts.mail.example.com');
        expect(extractDomain('https://mail.example.com/')).toBe('mail.example.com');
    });

    it('handles two-character ccTLDs', () => {
        expect(extractDomain('https://iqos.ru/')).toBe('iqos.ru');
        expect(extractDomain('https://www.iqos.ru/some/path')).toBe('iqos.ru');
    });

    it('does NOT collapse multi-part public suffixes to the suffix', () => {
        // The old last-two-labels logic returned "co.uk" here, which would match
        // every unrelated ".co.uk" entry via the agent's substring search.
        expect(extractDomain('https://victim.co.uk/')).toBe('victim.co.uk');
        expect(extractDomain('https://www.victim.co.uk/login')).toBe('victim.co.uk');
    });

    it('handles path and query string', () => {
        expect(extractDomain('https://example.com/path?q=1#hash')).toBe('example.com');
    });

    it('returns null for invalid URLs', () => {
        expect(extractDomain('not a url')).toBeNull();
    });

    it('returns empty string for about: pages (callers guard these)', () => {
        expect(extractDomain('about:blank')).toBe('');
    });

    it('returns the bare hostname for chrome: pages (callers guard these)', () => {
        expect(extractDomain('chrome://newtab/')).toBe('newtab');
    });
});
