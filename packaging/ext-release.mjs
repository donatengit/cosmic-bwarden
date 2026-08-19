// Pure release logic for the browser-extension signing pipeline.
//
// Everything decision-shaped lives here as pure, dependency-free functions so
// it can be unit-tested offline (node --test): the version preflight, the
// Firefox updates.json builder, sha256, and update_url injection. The thin
// CLI at the bottom only gathers facts from git/env/fs and prints results;
// network and AMO credentials are required only by the `web-ext sign` call in
// the justfile, never here.

import { createHash } from "node:crypto";
import {
  createReadStream,
  readFileSync,
  writeFileSync,
  existsSync,
  readdirSync,
  mkdirSync,
  openSync,
  closeSync,
  constants as fsConstants,
  lstatSync,
  realpathSync,
} from "node:fs";
import { execFileSync } from "node:child_process";
import { join, resolve, sep, dirname } from "node:path";

// ---------------------------------------------------------------------------
// Version helpers

export function stripLeadingV(tag) {
  return tag.replace(/^v/, "");
}

/**
 * A bare version in AMO's manifest format: one to four dot-separated integers
 * (e.g. 2026.8.0 for releases, 2026.8.19.1233 for timestamped dev signs).
 */
export function isValidVersion(version) {
  // Mirrors AMO's version regex: components are integers without leading
  // zeros ("0" itself is allowed).
  return /^(0|[1-9][0-9]*)(\.(0|[1-9][0-9]*)){0,3}$/.test(version);
}

/**
 * Timestamp version for dev signing: YYYY.M.D.mmm where mmm is minutes since
 * midnight (0-1439). Every component stays within the 0-65535 addons-linter
 * cap and never has a leading zero, so the format is valid for AMO, web-ext
 * lint, and Chrome. Minute resolution: two signs within the same minute
 * collide and are refused by the dist/ duplicate guard.
 */
export function timestampVersion(now = new Date()) {
  const minutesOfDay = now.getHours() * 60 + now.getMinutes();
  return `${now.getFullYear()}.${now.getMonth() + 1}.${now.getDate()}.${minutesOfDay}`;
}

/**
 * Returns a NEW manifest object with `version` set to `version`. Dev-mode
 * signing signs the CURRENT files under a unique timestamp version, so an
 * existing (repo) version is overwritten rather than treated as drift.
 */
export function applyVersion(manifest, version) {
  const result = JSON.parse(JSON.stringify(manifest));
  const injected = result.version !== version;
  result.version = version;
  return { manifest: result, injected };
}

// ---------------------------------------------------------------------------
// sha256 (streamed, so large XPIs are not slurped into memory)

export async function sha256File(filePath) {
  const hash = createHash("sha256");
  const stream = createReadStream(filePath);
  for await (const chunk of stream) {
    hash.update(chunk);
  }
  return hash.digest("hex");
}

// ---------------------------------------------------------------------------
// update_url injection into the staged manifest

export function normalizeBaseUrl(baseUrl) {
  const trimmed = String(baseUrl ?? "").trim();
  if (!trimmed) {
    throw new Error("EXT_UPDATE_BASE_URL is empty");
  }
  // This value is baked into the signed update_url and every update_link, so
  // a permissive scheme/userinfo/query would re-point the whole update
  // channel. https only, no embedded credentials, query, or fragment.
  let parsed;
  try {
    parsed = new URL(trimmed);
  } catch {
    throw new Error(`EXT_UPDATE_BASE_URL is not a valid URL: "${trimmed}"`);
  }
  if (parsed.protocol !== "https:") {
    throw new Error(`EXT_UPDATE_BASE_URL must be an https:// URL (got "${trimmed}")`);
  }
  if (parsed.username || parsed.password || parsed.search || parsed.hash) {
    throw new Error(
      `EXT_UPDATE_BASE_URL must not contain credentials, a query, or a fragment (got "${trimmed}")`
    );
  }
  return trimmed.replace(/\/+$/, "");
}

export function computeUpdateUrl(baseUrl) {
  return `${normalizeBaseUrl(baseUrl)}/updates.json`;
}

/**
 * Returns a NEW manifest object with `browser_specific_settings.gecko.update_url`
 * set to `<baseUrl>/updates.json`; `injected` is true when the key was added
 * or rewritten. Throws when the manifest already hardcodes a *different*
 * update_url — signing with a stale baked-in URL would point installed
 * clients at the wrong update server, so drift is a hard error.
 */
export function applyUpdateUrl(manifest, baseUrl) {
  const expected = computeUpdateUrl(baseUrl);
  const result = JSON.parse(JSON.stringify(manifest));
  const gecko = result.browser_specific_settings?.gecko;
  if (!gecko || typeof gecko !== "object" || Array.isArray(gecko)) {
    throw new Error("manifest.json is missing browser_specific_settings.gecko");
  }
  const existing = gecko.update_url;
  if (existing !== undefined && existing !== null && existing !== expected) {
    throw new Error(
      `manifest.json hardcodes update_url "${existing}" but EXT_UPDATE_BASE_URL implies "${expected}"`
    );
  }
  const injected = existing !== expected;
  gecko.update_url = expected;
  return { manifest: result, injected };
}

// ---------------------------------------------------------------------------
// Version preflight

/**
 * Pure decision over gathered facts. `canonicalTag` is the raw
 * `git describe --tags --exact-match` output (null when HEAD is not on a tag);
 * `distXpiVersions` / `updatesJsonVersions` are the versions already shipped
 * in dist/, gathered by the caller.
 */
export function checkVersionPreflight({
  manifestVersion,
  canonicalTag,
  dirty,
  allowDirty = false,
  updatesJsonVersions = [],
  distXpiVersions = [],
}) {
  const errors = [];
  let version = null;

  if (!canonicalTag) {
    errors.push(
      "HEAD is not on a release tag (vYYYY.MM.P) — create or check out the release tag before signing"
    );
  } else if (!/^v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)$/.test(canonicalTag)) {
    errors.push(
      `release tag "${canonicalTag}" does not match vYYYY.MM.P (a leading "v" followed by three dot-separated integers)`
    );
  } else {
    // Only trust version-derived checks (filename, manifest match, dist
    // duplicates) when the tag itself is well-formed — a malformed tag would
    // otherwise produce a misleading second error.
    version = stripLeadingV(canonicalTag);
    if (manifestVersion !== version) {
      errors.push(
        `manifest.json version "${manifestVersion}" does not match release tag version "${version}"`
      );
    }
    if (updatesJsonVersions.includes(version)) {
      errors.push(`version ${version} already exists in dist/updates.json (AMO rejects duplicate versions)`);
    }
    if (distXpiVersions.includes(version)) {
      errors.push(`dist/cosmic-bwarden-${version}.xpi already exists — refusing to re-sign`);
    }
  }

  if (dirty && !allowDirty) {
    errors.push("working tree is dirty — commit or set ALLOW_DIRTY=1 to override");
  }

  return { ok: errors.length === 0, errors, version };
}

// ---------------------------------------------------------------------------
// Firefox updates.json (self-hosted update manifest)

export function entryFor({ version, baseUrl, sha256Hex }) {
  return {
    version,
    update_link: `${normalizeBaseUrl(baseUrl)}/cosmic-bwarden-${version}.xpi`,
    update_hash: `sha256:${sha256Hex}`,
  };
}

/**
 * Builds a Firefox updates.json from `existing` (parsed file, or null) plus a
 * new entry. Newest version first; every pre-existing entry — and any other
 * addon id in the file — is preserved verbatim. Throws on a duplicate
 * version: the preflight already refuses those, this is the pure safety net.
 */
export function buildUpdatesJson({ addonId, existing = null, newEntry }) {
  const current = existing && typeof existing === "object" ? existing : {};
  const currentAddons =
    current.addons && typeof current.addons === "object" ? current.addons : {};

  const previous = Array.isArray(currentAddons[addonId]?.updates)
    ? currentAddons[addonId].updates
    : [];
  if (previous.some((entry) => entry.version === newEntry.version)) {
    throw new Error(`version ${newEntry.version} already exists in dist/updates.json`);
  }

  return {
    ...current,
    addons: {
      ...currentAddons,
      [addonId]: {
        ...(currentAddons[addonId] ?? {}),
        updates: [newEntry, ...previous],
      },
    },
  };
}

/**
 * Dev-mode preflight: no tag/dirty/manifest checks (the current files are what
 * gets signed, under a freshly generated version) — only the checks that
 * protect AMO from a duplicate submission.
 */
export function checkDevVersion({ version, updatesJsonVersions = [], distXpiVersions = [] }) {
  const errors = [];
  if (updatesJsonVersions.includes(version)) {
    errors.push(`version ${version} already exists in dist/updates.json (AMO rejects duplicate versions)`);
  }
  if (distXpiVersions.includes(version)) {
    errors.push(`dist/cosmic-bwarden-${version}.xpi already exists — refusing to re-sign`);
  }
  return { ok: errors.length === 0, errors, version };
}

// ---------------------------------------------------------------------------
// Thin CLI: gathers facts, delegates all decisions to the functions above.

function usage() {
  return `usage: node ext-release.mjs <command>

commands:
  inject-version <manifest-path> <version>      rewrite staged manifest version (dev signing)
  inject-update-url <manifest-path> <base-url>   rewrite staged manifest (stdout: "injected" | "unchanged")
  preflight [--dev] [--allow-dirty]             dev: timestamp version + duplicate checks; default: strict tag/version/dist preflight (stdout: the version)
  updates-json <xpi-path> <base-url> <version> [out-path]  merge dist/updates.json (default out: dist/updates.json)
  finalize-sign <artifacts-dir> <version> <dist-dir>
  sha256 <path>                                  print hex digest`;
}

function repoRoot() {
  // packaging/ext-release.mjs → repo root is one level up. Resolving from the
  // script's own location keeps this correct regardless of the invoking cwd.
  // No trailing slash, so withinRepo's prefix check is exact.
  return decodeURIComponent(new URL("..", import.meta.url).pathname).replace(/\/+$/, "");
}

/**
 * Confine a CLI-provided path to the repo root — refuses ../-escapes AND
 * symlinks: the deepest existing ancestor is realpath'd, so a symlinked
 * dist/ or target/ext-stage/ cannot redirect writes outside the repo.
 */
function withinRepo(path) {
  const abs = resolve(repoRoot(), path);
  let probe = abs;
  while (!existsSync(probe)) {
    probe = dirname(probe);
  }
  const real = realpathSync(probe) + abs.slice(probe.length);
  if (real !== repoRoot() && !real.startsWith(repoRoot() + sep)) {
    throw new Error(`refusing to write outside the repository: ${path}`);
  }
  return abs;
}

function gitOutput(args) {
  try {
    return execFileSync("git", args, { encoding: "utf8", cwd: repoRoot() }).trim();
  } catch {
    return null;
  }
}

function readManifest(path) {
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    throw new Error(`cannot read manifest at ${path}: ${err.message}`);
  }
}

function readUpdatesJson(path) {
  if (!existsSync(path)) {
    return null;
  }
  try {
    return JSON.parse(readFileSync(path, "utf8"));
  } catch (err) {
    throw new Error(`cannot parse ${path}: ${err.message}`);
  }
}

function versionsInUpdatesJson(doc) {
  const found = [];
  for (const id of Object.keys(doc?.addons ?? {})) {
    for (const entry of doc.addons[id]?.updates ?? []) {
      if (entry.version) {
        found.push(entry.version);
      }
    }
  }
  return found;
}

function versionsInDist(distDir) {
  if (!existsSync(distDir)) {
    return [];
  }
  const found = [];
  for (const name of readdirSync(distDir)) {
    const match = /^cosmic-bwarden-(.+)\.xpi$/.exec(name);
    if (match) {
      found.push(match[1]);
    }
  }
  return found;
}

function preflight(args) {
  const dev = args.includes("--dev");
  const allowDirty = args.includes("--allow-dirty");
  const distXpiVersions = versionsInDist(join(repoRoot(), "dist"));
  const updatesJsonVersions = versionsInUpdatesJson(
    readUpdatesJson(join(repoRoot(), "dist/updates.json"))
  );

  const result = dev
    ? checkDevVersion({ version: timestampVersion(), updatesJsonVersions, distXpiVersions })
    : checkVersionPreflight({
        manifestVersion: readManifest(join(repoRoot(), "browser-extension/manifest.json")).version,
        canonicalTag: gitOutput(["describe", "--tags", "--exact-match", "HEAD"]),
        dirty: (gitOutput(["status", "--porcelain"]) ?? "") !== "",
        allowDirty,
        updatesJsonVersions,
        distXpiVersions,
      });

  if (!result.ok) {
    for (const error of result.errors) {
      console.error(`preflight: ${error}`);
    }
    process.exit(1);
  }
  console.log(result.version);
}

function injectVersion(args) {
  const [manifestPath, version] = args;
  if (!isValidVersion(version)) {
    throw new Error(`invalid version: "${version}"`);
  }
  const confined = withinRepo(manifestPath);
  const { manifest, injected } = applyVersion(readManifest(confined), version);
  writeFileSync(confined, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(injected ? "injected" : "unchanged");
}

function injectUpdateUrl(args) {
  const [manifestPath, baseUrl] = args;
  const confined = withinRepo(manifestPath);
  const { manifest, injected } = applyUpdateUrl(readManifest(confined), baseUrl);
  writeFileSync(confined, `${JSON.stringify(manifest, null, 2)}\n`);
  console.log(injected ? "injected" : "unchanged");
}

async function updatesJson(args) {
  const [xpiPath, baseUrl, version, outPath] = args;
  if (!isValidVersion(version)) {
    throw new Error(`invalid version: "${version}"`);
  }
  const digest = await sha256File(xpiPath);
  const manifest = readManifest(join(repoRoot(), "browser-extension/manifest.json"));
  const addonId = manifest.browser_specific_settings?.gecko?.id;
  if (!addonId) {
    throw new Error("manifest.json is missing browser_specific_settings.gecko.id");
  }
  const out = withinRepo(outPath ?? "dist/updates.json");
  const doc = buildUpdatesJson({
    addonId,
    existing: readUpdatesJson(out),
    newEntry: entryFor({ version, baseUrl, sha256Hex: digest }),
  });
  mkdirSync(join(out, ".."), { recursive: true });
  writeFileSync(out, `${JSON.stringify(doc, null, 2)}\n`);
  console.log(out);
}

function finalizeSign(args) {
  const [artifactsDir, version, distDir] = args;
  const artifactsAbs = withinRepo(artifactsDir);
  const dest = join(withinRepo(distDir), `cosmic-bwarden-${version}.xpi`);
  const entries = readdirSync(artifactsAbs).filter(
    (name) => name.endsWith(".xpi") && lstatSync(join(artifactsAbs, name)).isFile()
  );
  if (entries.length !== 1) {
    throw new Error(
      `expected exactly one .xpi in ${artifactsDir}, found ${entries.length}: ${entries.join(", ")}`
    );
  }
  // The version ends up in a filename — only the preflight-shaped form is
  // acceptable (the justfile flow can only produce this; direct CLI calls
  // must not be able to ../-escape via the version argument).
  if (!isValidVersion(version)) {
    throw new Error(`invalid version for the artifact filename: "${version}"`);
  }
  // O_NOFOLLOW binds the read to the inode we just stat'd: a symlink swapped
  // in afterwards fails the open instead of being followed.
  const fd = openSync(join(artifactsAbs, entries[0]), fsConstants.O_RDONLY | fsConstants.O_NOFOLLOW);
  let content;
  try {
    content = readFileSync(fd);
  } finally {
    closeSync(fd);
  }
  mkdirSync(withinRepo(distDir), { recursive: true });
  writeFileSync(dest, content);
  console.log(dest);
}

async function main() {
  const [, , command, ...args] = process.argv;
  switch (command) {
    case "inject-version":
      injectVersion(args);
      break;
    case "inject-update-url":
      injectUpdateUrl(args);
      break;
    case "preflight":
      preflight(args);
      break;
    case "updates-json":
      await updatesJson(args);
      break;
    case "finalize-sign":
      finalizeSign(args);
      break;
    case "sha256":
      console.log(await sha256File(args[0]));
      break;
    default:
      console.error(usage());
      process.exit(2);
  }
}

// Only run the CLI when executed directly; importing from tests must not run
// anything. process.argv[1] is the script path under both `node script.mjs`
// and a justfile recipe invocation.
if (process.argv[1]?.endsWith("ext-release.mjs")) {
  main().catch((err) => {
    console.error(`ext-release: ${err.message}`);
    process.exit(1);
  });
}
