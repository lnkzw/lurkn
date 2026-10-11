#!/usr/bin/env node
/**
 * One version, three files, and the changelog that goes with it.
 *
 *   node scripts/version.mjs 0.2.0          set 0.2.0 everywhere, date the changelog
 *   node scripts/version.mjs --check 0.2.0  fail unless every file says 0.2.0
 *   node scripts/version.mjs --notes 0.2.0  print that version's changelog section
 *
 * The version lives in Cargo.toml (the workspace, which every crate inherits),
 * package.json and src-tauri/tauri.conf.json. Tauri reads the last one for the
 * installers, so a tag that disagrees with it would ship a mislabelled build;
 * the release workflow runs --check before it builds anything.
 *
 * It never commits or tags. It prints what to run, so a person does.
 */

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const path = (file) => join(root, file);
const read = (file) => readFileSync(path(file), 'utf8');

// 0.y.z with nothing after it: the Windows installer cannot carry a
// pre-release tag, and 0.x already says "not stable yet".
const SEMVER = /^\d+\.\d+\.\d+$/;

function fail(message) {
  console.error(`error: ${message}`);
  process.exit(1);
}

function versions() {
  const cargo = read('Cargo.toml').match(/\[workspace\.package\][^[]*?\nversion = "([^"]+)"/);
  return {
    'Cargo.toml': cargo?.[1],
    'package.json': JSON.parse(read('package.json')).version,
    'src-tauri/tauri.conf.json': JSON.parse(read('src-tauri/tauri.conf.json')).version,
  };
}

function notes(version) {
  const changelog = read('CHANGELOG.md');
  const heading = new RegExp(`^## \\[${version.replaceAll('.', '\\.')}\\].*$`, 'm');
  const start = changelog.search(heading);
  if (start < 0) fail(`CHANGELOG.md has no section for ${version}`);
  const rest = changelog.slice(start).split('\n').slice(1).join('\n');
  const end = rest.search(/^## \[/m);
  return (end < 0 ? rest : rest.slice(0, end)).replace(/\n\[[^\]]+\]: .*$/gm, '').trim();
}

function check(version) {
  const wrong = Object.entries(versions()).filter(([, found]) => found !== version);
  for (const [file, found] of wrong) console.error(`${file} says ${found ?? 'nothing'}`);
  if (wrong.length > 0) fail(`not every file is at ${version}`);
  notes(version);
  console.log(`ok: ${version} everywhere, with a changelog section`);
}

function bump(version) {
  const cargo = read('Cargo.toml').replace(
    /(\[workspace\.package\][^[]*?\nversion = ")[^"]+(")/,
    `$1${version}$2`,
  );
  writeFileSync(path('Cargo.toml'), cargo);

  // Text replacement rather than a JSON round trip, so the files keep their
  // formatting and the diff is one line each.
  for (const file of ['package.json', 'src-tauri/tauri.conf.json']) {
    const text = read(file).replace(/("version":\s*")[^"]+(")/, `$1${version}$2`);
    writeFileSync(path(file), text);
  }

  const changelog = read('CHANGELOG.md');
  if (!changelog.includes('## [Unreleased]')) fail('CHANGELOG.md has no [Unreleased] section');
  if (changelog.includes(`## [${version}]`)) fail(`CHANGELOG.md already has ${version}`);
  const today = new Date().toISOString().slice(0, 10);
  writeFileSync(
    path('CHANGELOG.md'),
    changelog.replace('## [Unreleased]', `## [Unreleased]\n\n## [${version}] - ${today}`),
  );

  // The lock file records the workspace's own versions too.
  execFileSync('cargo', ['update', '--workspace', '--quiet'], { cwd: root, stdio: 'inherit' });

  check(version);
  console.log(`
Next:
  git commit -am "release: ${version}"
  git tag v${version}
  git push && git push origin v${version}

The tag builds a draft release on GitHub; publish it once the downloads check out.`);
}

const [flag, value] = process.argv.slice(2);
if (flag === '--check' || flag === '--notes') {
  if (!value || !SEMVER.test(value)) fail(`${flag} needs a version like 0.2.0`);
  if (flag === '--check') check(value);
  else console.log(notes(value));
} else {
  if (!flag || !SEMVER.test(flag)) fail('usage: version.mjs <0.y.z> | --check <0.y.z> | --notes <0.y.z>');
  bump(flag);
}
