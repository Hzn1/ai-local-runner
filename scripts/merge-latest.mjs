#!/usr/bin/env node
/**
 * Merge per-platform `latest-*.json` fragments produced by scripts/release.sh
 * into one updater manifest. Usage:
 *
 *   node scripts/merge-latest.mjs release-artifacts > latest.json
 */
import { readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';

const dir = process.argv[2] ?? 'release-artifacts';
const files = readdirSync(dir).filter(
  (name) => name.startsWith('latest-') && name.endsWith('.json'),
);

if (files.length === 0) {
  console.error(`no latest-*.json fragments in ${dir}`);
  process.exit(1);
}

const merged = {
  version: null,
  notes: process.env.RELEASE_NOTES ?? '',
  pub_date: null,
  platforms: {},
};

for (const file of files) {
  const fragment = JSON.parse(readFileSync(join(dir, file), 'utf8'));
  if (merged.version && fragment.version !== merged.version) {
    console.error(
      `version mismatch: ${merged.version} vs ${fragment.version} (${file})`,
    );
    process.exit(1);
  }
  merged.version = fragment.version;
  merged.pub_date =
    !merged.pub_date || fragment.pub_date > merged.pub_date
      ? fragment.pub_date
      : merged.pub_date;
  if (fragment.notes) merged.notes = fragment.notes;
  Object.assign(merged.platforms, fragment.platforms);
}

if (!merged.version || Object.keys(merged.platforms).length === 0) {
  console.error('merged manifest is incomplete');
  process.exit(1);
}

process.stdout.write(`${JSON.stringify(merged, null, 2)}\n`);
