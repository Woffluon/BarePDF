export type ReleaseAssetType = 'installer' | 'portable' | 'checksum';

const GITHUB_HOST = 'github.com';
const TRUSTED_ASSET_HOSTS = new Set([
  'objects.githubusercontent.com',
  'release-assets.githubusercontent.com',
]);
const BAREPDF_REPO_PATH = '/Woffluon/BarePDF';
const GITHUB_RELEASE_ASSET_PREFIX = '/github-production-release-asset';

function isApprovedGitHubPath(hostname: string, pathname: string): boolean {
  if (/(?:%2e|%2f|%5c|\/\/)/i.test(pathname)) return false;

  if (hostname === GITHUB_HOST) {
    return pathname === BAREPDF_REPO_PATH || pathname.startsWith(`${BAREPDF_REPO_PATH}/`);
  }

  if (TRUSTED_ASSET_HOSTS.has(hostname)) {
    return (
      pathname === BAREPDF_REPO_PATH
      || pathname.startsWith(`${BAREPDF_REPO_PATH}/`)
      || pathname.startsWith(GITHUB_RELEASE_ASSET_PREFIX)
    );
  }

  return false;
}

export function trustedGitHubUrl(value: unknown): string | null {
  if (typeof value !== 'string') return null;

  try {
    const url = new URL(value);
    if (
      url.protocol !== 'https:'
      || !isApprovedGitHubPath(url.hostname, url.pathname)
      || url.username
      || url.password
      || url.port
      || url.search
      || url.hash
    ) return null;
    return url.href;
  } catch {
    return null;
  }
}

export function releaseAssetNames(tag: string): Record<ReleaseAssetType, string> | null {
  const version = /^v(\d+\.\d+\.\d+)$/.exec(tag)?.[1];
  if (!version) return null;

  return {
    installer: `BarePDF-Setup-x64-v${version}.exe`,
    portable: `BarePDF-Portable-x64-v${version}.zip`,
    checksum: `BarePDF-v${version}-SHA256SUMS.txt`,
  };
}

export function releaseAssetType(name: string, tag: string): ReleaseAssetType | null {
  const names = releaseAssetNames(tag);
  if (!names) return null;

  for (const type of ['installer', 'portable', 'checksum'] as const) {
    if (names[type] === name) return type;
  }
  return null;
}

export function isReleaseAssetUrl(
  value: string,
  owner: string,
  repository: string,
  tag: string,
  name: string,
): boolean {
  const url = trustedGitHubUrl(value);
  if (!url || releaseAssetType(name, tag) === null) return false;
  const expectedPath = `/${owner}/${repository}/releases/download/${encodeURIComponent(tag)}/${encodeURIComponent(name)}`;
  const parsedUrl = new URL(url);
  return parsedUrl.hostname === GITHUB_HOST && parsedUrl.pathname === expectedPath;
}

export function isReleasePageUrl(
  value: string,
  owner: string,
  repository: string,
  tag: string,
): boolean {
  const url = trustedGitHubUrl(value);
  if (!url) return false;
  const parsedUrl = new URL(url);
  return parsedUrl.hostname === GITHUB_HOST
    && parsedUrl.pathname === `/${owner}/${repository}/releases/tag/${encodeURIComponent(tag)}`;
}

export function isCommitUrl(
  value: string,
  owner: string,
  repository: string,
  sha: string,
): boolean {
  const url = trustedGitHubUrl(value);
  if (!url) return false;
  const parsedUrl = new URL(url);
  return parsedUrl.hostname === GITHUB_HOST
    && parsedUrl.pathname === `/${owner}/${repository}/commit/${sha}`;
}
