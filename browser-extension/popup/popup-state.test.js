import { describe, it, expect } from 'vitest';
import { readFileSync } from 'fs';
import { fileURLToPath } from 'url';
import path from 'path';

const source = readFileSync(
    path.join(path.dirname(fileURLToPath(import.meta.url)), 'popup-state.js'),
    'utf8'
);
const match = source.match(/function isUnchangedSecretField\([\s\S]*?\n\}/);
if (!match) throw new Error('isUnchangedSecretField not found');
const isUnchangedSecretField = new Function(`${match[0]}\nreturn isUnchangedSecretField;`)();

describe('isUnchangedSecretField', () => {
    it('omits a password that still matches the stored vault value', () => {
        expect(isUnchangedSecretField('password', 'hunter2', 'hunter2')).toBe(true);
    });
    it('keeps a user-typed password change', () => {
        expect(isUnchangedSecretField('password', 'new-secret', 'hunter2')).toBe(false);
    });
    it('never treats username as a secret to omit', () => {
        expect(isUnchangedSecretField('username', 'ada', 'ada')).toBe(false);
    });
});
