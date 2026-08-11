export type ReleaseAssetType = 'installer' | 'portable' | 'checksum';

const GITHUB_HOST = 'github.com';
const GITHUB_CONTENT_HOST_SUFFIX = '.githubusercontent.com';

export function trustedGitHubUrl(value: unknown): string | null {
  if (typeof value !== 'string') return null;

  try {
    const url = new URL(value);
    const trustedHost = url.hostname === GITHUB_HOST || url.hostname.endsWith(GITHUB_CONTENT_HOST_SUFFIX);
    if (
      url.protocol !== 'https:'
      || !trustedHost
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

export function releaseAssetNames(tag: string): Record<ReleaseAssetType, readonly [string, string]> | null {
  const version = /^v(\d+\.\d+\.\d+)$/.exec(tag)?.[1];
  if (!version) return null;

  return {
    installer: [`BarePDF-Setup-x64-v${version}.exe`, 'BarePDF-Setup-x64.exe'],
    portable: [`BarePDF-Portable-x64-v${version}.zip`, 'BarePDF-Portable-x64.zip'],
    checksum: [`BarePDF-v${version}-SHA256SUMS.txt`, 'BarePDF-SHA256SUMS.txt'],
  };
}

export function releaseAssetType(name: string, tag: string): ReleaseAssetType | null {
  const names = releaseAssetNames(tag);
  if (!names) return null;

  for (const type of ['installer', 'portable', 'checksum'] as const) {
    if (names[type].includes(name)) return type;
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
