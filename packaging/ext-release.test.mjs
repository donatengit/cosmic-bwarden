// Unit tests for the pure release logic — no network, no AMO credentials, no
// git invocation (facts are fed in). Run with: node --test packaging/

import { test } from "node:test";
import assert from "node:assert/strict";
import { mkdtempSync, writeFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

import {
  stripLeadingV,
  isValidVersion,
  timestampVersion,
  applyVersion,
  normalizeBaseUrl,
  computeUpdateUrl,
  applyUpdateUrl,
  checkVersionPreflight,
  checkDevVersion,
  entryFor,
  buildUpdatesJson,
  sha256File,
} from "./ext-release.mjs";

const ADDON_ID = "cosmic-bwarden@enikeev.com";

test("isValidVersion accepts one to four dot-separated integers (AMO format)", () => {
  for (const good of ["0.1.0", "2026.8.0", "9.99.999", "2026.8.19.1233", "1", "0"]) {
    assert.equal(isValidVersion(good), true, good);
  }
  for (const bad of ["", "1.2.3.4.5", "a.b.c", "1..2", "1.2.", "../1.2.3", "v1.2.3", "1.02"]) {
    assert.equal(isValidVersion(bad), false, bad);
  }
});

test("timestampVersion is YYYY.M.D.mmm with no leading zeros and stays under 65535", () => {
  const at = (date) => timestampVersion(date);
  // 2026-08-19 12:33 → 2026.8.19.753 (12*60+33)
  assert.equal(at(new Date(2026, 7, 19, 12, 33, 59)), "2026.8.19.753");
  // Midnight → minutes-of-day 0 (a bare "0" component, valid per AMO's regex).
  assert.equal(at(new Date(2026, 0, 5, 0, 0, 0)), "2026.1.5.0");
  // 23:59 → 1439, the maximum component value.
  assert.equal(at(new Date(2026, 11, 31, 23, 59, 59)), "2026.12.31.1439");
  // Every component is a plain integer without leading zeros.
  for (const date of [new Date(2026, 7, 1, 0, 0), new Date(2026, 10, 5, 9, 7)]) {
    const v = timestampVersion(date);
    assert.match(v, /^\d{4}\.\d{1,2}\.\d{1,2}\.\d{1,4}$/);
    assert.equal(isValidVersion(v), true, v);
    const [, , , minutes] = v.split(".").map(Number);
    assert.ok(minutes <= 65535);
  }
});

test("applyVersion overwrites the version on a copy without mutating the input", () => {
  const manifest = structuredClone(baseManifest);
  const { manifest: out, injected } = applyVersion(manifest, "2026.8.19.753");
  assert.equal(injected, true);
  assert.equal(out.version, "2026.8.19.753");
  assert.equal(manifest.version, "2026.8.0");
  const same = applyVersion(out, "2026.8.19.753");
  assert.equal(same.injected, false);
});

test("dev preflight passes for a fresh version and refuses dist/ duplicates", () => {
  const ok = checkDevVersion({ version: "2026.8.19.753" });
  assert.equal(ok.ok, true);
  assert.equal(ok.version, "2026.8.19.753");

  const dupJson = checkDevVersion({
    version: "2026.8.19.753",
    updatesJsonVersions: ["2026.8.19.753"],
  });
  assert.equal(dupJson.ok, false);
  assert.match(dupJson.errors.join("\n"), /already exists in dist\/updates.json/);

  const dupXpi = checkDevVersion({
    version: "2026.8.19.753",
    distXpiVersions: ["2026.8.19.753"],
  });
  assert.equal(dupXpi.ok, false);
  assert.match(dupXpi.errors.join("\n"), /refusing to re-sign/);

  // Unrelated dist versions do not block.
  assert.equal(
    checkDevVersion({
      version: "2026.8.19.753",
      updatesJsonVersions: ["2026.8.19.752"],
      distXpiVersions: ["2026.8.19.752"],
    }).ok,
    true
  );
});

test("stripLeadingV removes only the leading v", () => {
  assert.equal(stripLeadingV("v2026.8.0"), "2026.8.0");
  assert.equal(stripLeadingV("2026.8.0"), "2026.8.0");
  assert.equal(stripLeadingV("v1.2.3"), "1.2.3");
});

test("normalizeBaseUrl trims and strips trailing slashes, rejects empty", () => {
  assert.equal(normalizeBaseUrl("https://updates.example.com/base/"), "https://updates.example.com/base");
  assert.equal(normalizeBaseUrl(" https://updates.example.com "), "https://updates.example.com");
  assert.equal(normalizeBaseUrl("https://x/base///"), "https://x/base");
  assert.throws(() => normalizeBaseUrl(""), /EXT_UPDATE_BASE_URL is empty/);
  assert.throws(() => normalizeBaseUrl(null), /EXT_UPDATE_BASE_URL is empty/);
});

test("normalizeBaseUrl requires https and forbids credentials/query/fragment", () => {
  assert.throws(() => normalizeBaseUrl("http://updates.example.com/base"), /must be an https:\/\/ URL/);
  assert.throws(() => normalizeBaseUrl("ftp://updates.example.com/base"), /must be an https:\/\/ URL/);
  assert.throws(() => normalizeBaseUrl("https://user:pass@updates.example.com"), /must not contain credentials/);
  assert.throws(() => normalizeBaseUrl("https://updates.example.com?x=1"), /must not contain credentials/);
  assert.throws(() => normalizeBaseUrl("https://updates.example.com#frag"), /must not contain credentials/);
  assert.throws(() => normalizeBaseUrl("not a url"), /not a valid URL/);
  // A port is legitimate.
  assert.equal(normalizeBaseUrl("https://updates.example.com:8443/base"), "https://updates.example.com:8443/base");
});

test("computeUpdateUrl appends /updates.json", () => {
  assert.equal(
    computeUpdateUrl("https://updates.example.com/cosmic-bwarden"),
    "https://updates.example.com/cosmic-bwarden/updates.json"
  );
});

const baseManifest = {
  manifest_version: 3,
  name: "COSMIC BWarden",
  version: "2026.8.0",
  browser_specific_settings: { gecko: { id: ADDON_ID, strict_min_version: "115.0" } },
};

test("applyUpdateUrl injects when absent and reports injected", () => {
  const { manifest, injected } = applyUpdateUrl(baseManifest, "https://updates.example.com/base");
  assert.equal(injected, true);
  assert.equal(manifest.browser_specific_settings.gecko.update_url, "https://updates.example.com/base/updates.json");
  // The input object must not be mutated.
  assert.equal(baseManifest.browser_specific_settings.gecko.update_url, undefined);
});

test("applyUpdateUrl is a no-op when the URL already matches", () => {
  const withUrl = structuredClone(baseManifest);
  withUrl.browser_specific_settings.gecko.update_url = "https://updates.example.com/base/updates.json";
  const { manifest, injected } = applyUpdateUrl(withUrl, "https://updates.example.com/base/");
  assert.equal(injected, false);
  assert.equal(manifest.browser_specific_settings.gecko.update_url, "https://updates.example.com/base/updates.json");
});

test("applyUpdateUrl throws on a hardcoded conflicting update_url (drift guard)", () => {
  const withUrl = structuredClone(baseManifest);
  withUrl.browser_specific_settings.gecko.update_url = "https://old.example.com/updates.json";
  assert.throws(
    () => applyUpdateUrl(withUrl, "https://updates.example.com/base"),
    /hardcodes update_url/
  );
});

test("applyUpdateUrl throws when gecko settings are missing", () => {
  const withoutGecko = structuredClone(baseManifest);
  delete withoutGecko.browser_specific_settings;
  assert.throws(() => applyUpdateUrl(withoutGecko, "https://x"), /missing browser_specific_settings.gecko/);
});

test("preflight passes on a clean tagged HEAD (the tag is the version)", () => {
  const result = checkVersionPreflight({
    canonicalTag: "v2026.8.0",
    dirty: false,
  });
  assert.equal(result.ok, true);
  assert.deepEqual(result.errors, []);
  assert.equal(result.version, "2026.8.0");
});

test("preflight fails when HEAD is not on a tag", () => {
  const result = checkVersionPreflight({
    canonicalTag: null,
    dirty: false,
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /not on a release tag/);
});

test("preflight fails when the tag does not match vYYYY.MM.P", () => {
  for (const badTag of ["2026.8.0", "v1.2", "va.b.c", "v2026/8/0", "v2026.08.0.1", "v2026.08.0"]) {
    const result = checkVersionPreflight({
      canonicalTag: badTag,
      dirty: false,
    });
    assert.equal(result.ok, false, `tag ${badTag} should be rejected`);
    assert.match(result.errors.join("\n"), /does not match vYYYY.MM.P/);
    // A malformed tag must not produce a misleading manifest-mismatch error.
    assert.equal(result.errors.length, 1);
  }
  // Three dot-separated integers after the v pass (calendar vYYYY.MM.P and
  // semver-style v0.1.0 are both valid here) when the manifest matches.
  for (const [goodTag, ver] of [["v2026.8.0", "2026.8.0"], ["v0.1.0", "0.1.0"]]) {
    const result = checkVersionPreflight({ canonicalTag: goodTag, dirty: false });
    assert.equal(result.ok, true, `tag ${goodTag} should pass`);
    assert.equal(result.version, ver);
  }
});

test("preflight fails on a dirty tree unless ALLOW_DIRTY", () => {
  const base = { canonicalTag: "v2026.8.0" };
  assert.equal(checkVersionPreflight({ ...base, dirty: true }).ok, false);
  assert.match(
    checkVersionPreflight({ ...base, dirty: true }).errors.join("\n"),
    /ALLOW_DIRTY=1/
  );
  assert.equal(checkVersionPreflight({ ...base, dirty: true, allowDirty: true }).ok, true);
});

test("preflight refuses a version already in dist/updates.json", () => {
  const result = checkVersionPreflight({
    canonicalTag: "v2026.8.0",
    dirty: false,
    updatesJsonVersions: ["2026.7.0", "2026.8.0"],
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /already exists in dist\/updates.json/);
});

test("preflight refuses a version whose XPI already exists in dist/", () => {
  const result = checkVersionPreflight({
    canonicalTag: "v2026.8.0",
    dirty: false,
    distXpiVersions: ["2026.8.0"],
  });
  assert.equal(result.ok, false);
  assert.match(result.errors.join("\n"), /refusing to re-sign/);
});

test("preflight ignores other versions in dist/", () => {
  const result = checkVersionPreflight({
    canonicalTag: "v2026.8.0",
    dirty: false,
    updatesJsonVersions: ["2026.7.0"],
    distXpiVersions: ["2026.7.0"],
  });
  assert.equal(result.ok, true);
});

test("entryFor defaults link_base to the update base and supports a linkBase override", () => {
  const base = "https://github.com/donatengit/cosmic-bwarden/releases/latest/download";
  const linkBase = "https://github.com/donatengit/cosmic-bwarden/releases/download/v2026.8.0";
  const entry = entryFor({ version: "2026.8.0", baseUrl: base, sha256Hex: "abc123" });
  assert.deepEqual(entry, {
    version: "2026.8.0",
    update_link: `${base}/cosmic-bwarden-2026.8.0.xpi`,
    update_hash: "sha256:abc123",
  });
  const scoped = entryFor({ version: "2026.8.0", baseUrl: base, sha256Hex: "abc123", linkBase });
  assert.deepEqual(scoped, {
    version: "2026.8.0",
    update_link: `${linkBase}/cosmic-bwarden-2026.8.0.xpi`,
    update_hash: "sha256:abc123",
  });
});

test("entryFor builds the exact Firefox update entry", () => {
  const entry = entryFor({
    version: "2026.8.0",
    baseUrl: "https://updates.example.com/base/",
    sha256Hex: "abc123",
  });
  assert.deepEqual(entry, {
    version: "2026.8.0",
    update_link: "https://updates.example.com/base/cosmic-bwarden-2026.8.0.xpi",
    update_hash: "sha256:abc123",
  });
});

test("buildUpdatesJson creates the manifest structure when none exists", () => {
  const doc = buildUpdatesJson({
    addonId: ADDON_ID,
    existing: null,
    newEntry: { version: "2026.8.0", update_link: "https://x/a.xpi", update_hash: "sha256:aa" },
  });
  assert.deepEqual(doc, {
    addons: {
      [ADDON_ID]: {
        updates: [
          { version: "2026.8.0", update_link: "https://x/a.xpi", update_hash: "sha256:aa" },
        ],
      },
    },
  });
});

test("buildUpdatesJson appends newest-first and preserves existing entries and other addon ids", () => {
  const existing = {
    addons: {
      [ADDON_ID]: {
        updates: [
          { version: "2026.7.0", update_link: "https://x/old.xpi", update_hash: "sha256:bb" },
        ],
      },
      "other@example.com": {
        updates: [{ version: "1.0.0", update_link: "https://x/o.xpi", update_hash: "sha256:cc" }],
      },
    },
  };
  const doc = buildUpdatesJson({
    addonId: ADDON_ID,
    existing,
    newEntry: { version: "2026.8.0", update_link: "https://x/new.xpi", update_hash: "sha256:aa" },
  });
  assert.deepEqual(doc.addons[ADDON_ID].updates.map((e) => e.version), ["2026.8.0", "2026.7.0"]);
  assert.deepEqual(doc.addons["other@example.com"], existing.addons["other@example.com"]);
  // The original object must not be mutated.
  assert.equal(existing.addons[ADDON_ID].updates.length, 1);
});

test("buildUpdatesJson throws on a duplicate version", () => {
  const existing = {
    addons: {
      [ADDON_ID]: {
        updates: [{ version: "2026.8.0", update_link: "https://x/a.xpi", update_hash: "sha256:aa" }],
      },
    },
  };
  assert.throws(
    () =>
      buildUpdatesJson({
        addonId: ADDON_ID,
        existing,
        newEntry: { version: "2026.8.0", update_link: "https://x/b.xpi", update_hash: "sha256:bb" },
      }),
    /already exists in dist\/updates.json/
  );
});

test("sha256File matches the known-answer digest of \"abc\"", async () => {
  const dir = mkdtempSync(join(tmpdir(), "ext-release-test-"));
  try {
    const path = join(dir, "abc.txt");
    writeFileSync(path, "abc");
    assert.equal(
      await sha256File(path),
      "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    // Empty file known-answer.
    const empty = join(dir, "empty.txt");
    writeFileSync(empty, "");
    assert.equal(
      await sha256File(empty),
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
