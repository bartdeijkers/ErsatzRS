import { copyFile, mkdir, readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import process from 'node:process';

const root = resolve(import.meta.dirname, '..', '..');
const files = [
  ['node_modules/hls.js/dist/hls.mjs', 'crates/ersatzrs-web/public/vendor/hls/hls.mjs'],
  ['node_modules/hls.js/LICENSE', 'crates/ersatzrs-web/public/vendor/hls/LICENSE'],
];
const check = process.argv.includes('--check');

for (const [sourceRelative, targetRelative] of files) {
  const source = resolve(root, sourceRelative);
  const target = resolve(root, targetRelative);
  if (check) {
    const [expected, actual] = await Promise.all([readFile(source), readFile(target)]);
    if (!expected.equals(actual)) {
      throw new Error(`${targetRelative} does not match hls.js 1.6.16`);
    }
  } else {
    await mkdir(dirname(target), { recursive: true });
    await copyFile(source, target);
  }
}
