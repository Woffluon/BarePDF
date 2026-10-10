import { repository, defaultReleaseFallback } from './repository.ts';
import type { ReleaseAsset } from './releases.ts';
import {
  isCommitUrl,
  isReleaseAssetUrl,
  isReleasePageUrl,
  releaseAssetNames,
  releaseAssetType,
  trustedGitHubUrl,
  type ReleaseAssetType,
} from './github-validation.ts';

interface ReleaseMetadata {
  tag: string;
  name: string;
  publishedAt: string | null;
  url: string;
  notes: string;
  assets: ReleaseAsset[];
}

export type GitHubRelease =
  | (ReleaseMetadata & { state: 'published'; publishedAt: string })
  | (ReleaseMetadata & { state: 'fallback'; publishedAt: null });

export interface GitHubCommit {
  sha: string;
  shortSha: string;
  message: string;
  subject: string;
  body: string | null;
  authorName: string | null;
  date: string | null;
  url: string;
}

export interface DownloadMetrics {
  totalDownloads: number;
  installerDownloads: number;
  portableDownloads: number;
  latestReleaseDownloads: number;
}

const GITHUB_API_BASE = 'https://api.github.com';
export const MAX_API_RESPONSE_BYTES = 2 * 1024 * 1024;

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function finiteSize(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : null;
}

export async function readBoundedJson(
  res: Response,
  maxBytes = MAX_API_RESPONSE_BYTES,
): Promise<unknown> {
  const contentLength = res.headers.get('content-length');
  if (contentLength !== null) {
    const declaredBytes = Number(contentLength);
    if (!Number.isFinite(declaredBytes) || declaredBytes < 0 || declaredBytes > maxBytes) {
      await res.body?.cancel().catch(() => {});
      throw new Error(`GitHub API response Content-Length (${contentLength}) exceeds limit (${maxBytes} bytes)`);
    }
  }

  if (!res.body) {
    throw new Error('GitHub API response body is empty');
  }

  const reader = res.body.getReader();
  const chunks: Uint8Array[] = [];
  let totalBytes = 0;

  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      if (value) {
        totalBytes += value.byteLength;
        if (totalBytes > maxBytes) {
          await reader.cancel().catch(() => {});
          throw new Error(`GitHub API response body exceeds limit (${maxBytes} bytes)`);
        }
        chunks.push(value);
      }
    }
  } finally {
    reader.releaseLock();
  }

  const merged = new Uint8Array(totalBytes);
  let offset = 0;
  for (const chunk of chunks) {
    merged.set(chunk, offset);
    offset += chunk.byteLength;
  }

  const text = new TextDecoder('utf-8', { fatal: true }).decode(merged);
  return JSON.parse(text);
}

function parseReleaseAssets(value: unknown, tag: string): ReleaseAsset[] {
  const expectedNames = releaseAssetNames(tag);
  if (!Array.isArray(value) || !expectedNames) return [];

  const candidates = value.flatMap((asset) => {
    if (!isRecord(asset)) return [];
    const name = stringValue(asset.name);
    const size = finiteSize(asset.size);
    const downloadUrl = trustedGitHubUrl(asset.browser_download_url);
    const type = name ? releaseAssetType(name, tag) : null;
    const downloadCount = typeof asset.download_count === 'number' && Number.isFinite(asset.download_count) && asset.download_count >= 0
      ? Math.trunc(asset.download_count)
      : 0;
    if (
      !name
      || size === null
      || !downloadUrl
      || !type
      || !isReleaseAssetUrl(downloadUrl, repository.owner, repository.name, tag, name)
    ) return [];

    return [{ name, size, downloadUrl, type, downloadCount }];
  });

  return (['installer', 'portable', 'checksum'] as const).flatMap((type: ReleaseAssetType) => {
    const asset = candidates.find((candidate) => candidate.name === expectedNames[type]);
    return asset ? [asset] : [];
  });
}

function getHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    'User-Agent': 'BarePDF-Website-Build',
    'Accept': 'application/vnd.github.v3+json',
  };

  const token = import.meta.env?.GITHUB_TOKEN ?? process.env.GITHUB_TOKEN;
  if (token) {
    headers['Authorization'] = `Bearer ${token}`;
  }

  return headers;
}

async function fetchWithTimeout(url: string, timeoutMs = 5000): Promise<Response> {
  const controller = new AbortController();
  const id = setTimeout(() => controller.abort(), timeoutMs);
  try {
    const res = await fetch(url, {
      headers: getHeaders(),
      redirect: 'error',
      signal: controller.signal,
    });
    return res;
  } finally {
    clearTimeout(id);
  }
}

export async function getLatestRelease(): Promise<GitHubRelease> {
  const url = `${GITHUB_API_BASE}/repos/${repository.owner}/${repository.name}/releases/latest`;
  try {
    const res = await fetchWithTimeout(url);
    if (!res.ok) {
      console.warn(`[GitHub API] Failed to fetch latest release (${res.status} ${res.statusText}). Using build fallback.`);
      return defaultReleaseFallback;
    }
    const data: unknown = await readBoundedJson(res);
    if (!isRecord(data) || data.draft !== false || data.prerelease !== false) return defaultReleaseFallback;

    const tag = stringValue(data.tag_name);
    const releaseUrl = trustedGitHubUrl(data.html_url);
    const publishedAt = stringValue(data.published_at);
    if (
      !tag
      || !releaseUrl
      || !publishedAt
      || !Number.isFinite(Date.parse(publishedAt))
      || !isReleasePageUrl(releaseUrl, repository.owner, repository.name, tag)
    ) return defaultReleaseFallback;
    const assets = parseReleaseAssets(data.assets, tag);
    if (assets.length !== 3) return defaultReleaseFallback;

    return {
      state: 'published',
      tag,
      name: stringValue(data.name) ?? tag,
      publishedAt,
      url: releaseUrl,
      notes: stringValue(data.body) ?? 'No release notes provided.',
      assets,
    };
  } catch (err) {
    console.warn(`[GitHub API] Error fetching latest release:`, err);
    return defaultReleaseFallback;
  }
}

export async function getDownloadMetrics(): Promise<DownloadMetrics | null> {
  const url = `${GITHUB_API_BASE}/repos/${repository.owner}/${repository.name}/releases?per_page=100`;
  try {
    const res = await fetchWithTimeout(url);
    if (!res.ok) {
      console.warn(`[GitHub API] Failed to fetch releases for download metrics (${res.status}). No mock numbers used.`);
      return null;
    }
    const data: unknown = await readBoundedJson(res);
    if (!Array.isArray(data)) {
      return null;
    }

    let installerCount = 0;
    let portableCount = 0;
    let latestCount = 0;
    let firstRelease = true;

    for (const release of data) {
      if (!isRecord(release) || release.draft === true || release.prerelease === true) {
        continue;
      }
      if (!Array.isArray(release.assets)) continue;

      let releaseInstaller = 0;
      let releasePortable = 0;

      for (const asset of release.assets) {
        if (!isRecord(asset)) continue;
        const name = stringValue(asset.name);
        const count = typeof asset.download_count === 'number' && Number.isFinite(asset.download_count) && asset.download_count >= 0
          ? Math.trunc(asset.download_count)
          : 0;

        if (name && /BarePDF-Setup-x64.*\.exe$/i.test(name)) {
          releaseInstaller += count;
        } else if (name && /BarePDF-Portable-x64.*\.zip$/i.test(name)) {
          releasePortable += count;
        }
      }

      installerCount += releaseInstaller;
      portableCount += releasePortable;

      if (firstRelease) {
        latestCount = releaseInstaller + releasePortable;
        firstRelease = false;
      }
    }

    const total = installerCount + portableCount;

    return {
      totalDownloads: total,
      installerDownloads: installerCount,
      portableDownloads: portableCount,
      latestReleaseDownloads: latestCount,
    };
  } catch (err) {
    console.warn(`[GitHub API] Error fetching download metrics:`, err);
    return null;
  }
}

export async function getRecentCommits(limit = 30): Promise<GitHubCommit[]> {
  const boundedLimit = Math.max(1, Math.min(100, Math.trunc(limit)));
  const url = `${GITHUB_API_BASE}/repos/${repository.owner}/${repository.name}/commits?sha=${encodeURIComponent(repository.defaultBranch)}&per_page=${boundedLimit}`;
  try {
    const res = await fetchWithTimeout(url);
    if (!res.ok) {
      console.warn(`[GitHub API] Failed to fetch commits (${res.status}). Using build fallback.`);
      return getFallbackCommits();
    }
    const data: unknown = await readBoundedJson(res);
    if (!Array.isArray(data)) {
      return getFallbackCommits();
    }

    const commits = data.flatMap((commit) => {
      if (!isRecord(commit) || !isRecord(commit.commit)) return [];
      const fullMsg = stringValue(commit.commit.message);
      const sha = stringValue(commit.sha);
      const url = trustedGitHubUrl(commit.html_url);
      if (
        !fullMsg
        || !sha
        || !/^[a-f0-9]{7,64}$/i.test(sha)
        || !url
        || !isCommitUrl(url, repository.owner, repository.name, sha)
      ) return [];
      const lines = fullMsg.split('\n');
      const subject = lines[0] || 'No commit message';
      const body = lines.slice(1).join('\n').trim() || null;
      const author = isRecord(commit.author) ? stringValue(commit.author.login) : null;
      const metadata = isRecord(commit.commit.author) ? commit.commit.author : null;

      return [{
        sha,
        shortSha: sha.substring(0, 7),
        message: fullMsg,
        subject,
        body,
        authorName: metadata ? stringValue(metadata.name) ?? author ?? 'Contributor' : author ?? 'Contributor',
        date: metadata ? stringValue(metadata.date) : null,
        url,
      }];
    });

    return commits.length > 0 ? commits : getFallbackCommits();
  } catch (err) {
    console.warn(`[GitHub API] Error fetching commits:`, err);
    return getFallbackCommits();
  }
}

function getFallbackCommits(): GitHubCommit[] {
  const sha = process.env.GITHUB_SHA ?? process.env.BUILD_COMMIT_SHA;
  if (!sha || !/^[a-f0-9]{7,64}$/i.test(sha)) return [];

  return [{
    sha,
    shortSha: sha.substring(0, 7),
    message: 'Current site build',
    subject: 'Current site build',
    body: null,
    authorName: process.env.GITHUB_ACTOR ?? 'BarePDF Build',
    date: null,
    url: `${repository.url}/commit/${sha}`,
  }];
}
