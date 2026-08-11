import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

let searchDirectory = dirname(fileURLToPath(import.meta.url));
let cargoManifest: string | undefined;

while (!cargoManifest) {
  const candidate = resolve(searchDirectory, 'Cargo.toml');
  if (existsSync(candidate)) {
    const content = readFileSync(candidate, 'utf8');
    if (/^\[workspace\.package\]\s*$/m.test(content)) cargoManifest = content;
  }

  const parent = dirname(searchDirectory);
  if (parent === searchDirectory) break;
  searchDirectory = parent;
}

if (!cargoManifest) {
  throw new Error('Cannot locate workspace Cargo.toml from website module');
}

const productVersion = cargoManifest.match(
  /\[workspace\.package\][\s\S]*?^version\s*=\s*"([^"]+)"/m,
)?.[1];

if (!productVersion) {
  throw new Error('Cannot read [workspace.package].version from Cargo.toml');
}

export const repository = {
  owner: 'Woffluon',
  name: 'BarePDF',
  motto: 'Bare, Fast, Yours.',
  url: 'https://github.com/Woffluon/BarePDF',
  defaultBranch: 'main',
  license: 'MIT',
  version: productVersion,
  description: 'Bare, Fast, Yours. Lightweight open-source PDF reader for Windows built with Rust and PDFium.',
};

export const defaultReleaseFallback = {
  state: 'fallback' as const,
  tag: `v${productVersion}`,
  name: `BarePDF v${productVersion}`,
  publishedAt: null,
  url: 'https://github.com/Woffluon/BarePDF/releases/latest',
  notes: 'GitHub release metadata was unavailable during this site build. Open GitHub Releases for current published downloads.',
  assets: [],
};
