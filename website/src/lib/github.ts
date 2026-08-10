import { repository, defaultReleaseFallback } from './repository';
import { classifyAsset, type ReleaseAsset } from './releases';

export interface GitHubRelease {
  tag: string;
  name: string;
  publishedAt: string | null;
  url: string;
  notes: string;
  assets: ReleaseAsset[];
}

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

const GITHUB_API_BASE = 'https://api.github.com';
const GITHUB_HOST = 'github.com';
const GITHUB_CONTENT_HOST_SUFFIX = '.githubusercontent.com';

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null;
}

function stringValue(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function finiteSize(value: unknown): number | null {
  return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

export function trustedGitHubUrl(value: unknown): string | null {
  if (typeof value !== 'string') return null;

  try {
    const url = new URL(value);
    const trustedHost = url.hostname === GITHUB_HOST || url.hostname.endsWith(GITHUB_CONTENT_HOST_SUFFIX);
    return url.protocol === 'https:' && trustedHost ? url.href : null;
  } catch {
    return null;
  }
}

function isReleaseAssetUrl(url: string, tag: string): boolean {
  const expectedPath = `/${repository.owner}/${repository.name}/releases/download/${encodeURIComponent(tag)}/`;
  return new URL(url).hostname === GITHUB_HOST && new URL(url).pathname.startsWith(expectedPath);
}

function isReleasePageUrl(url: string, tag: string): boolean {
  const expectedPath = `/${repository.owner}/${repository.name}/releases/tag/${encodeURIComponent(tag)}`;
  const parsedUrl = new URL(url);
  return parsedUrl.hostname === GITHUB_HOST && parsedUrl.pathname === expectedPath;
}

function parseReleaseAssets(value: unknown, tag: string): ReleaseAsset[] {
  if (!Array.isArray(value)) return [];

  return value.flatMap((asset) => {
    if (!isRecord(asset)) return [];
    const name = stringValue(asset.name);
    const size = finiteSize(asset.size);
    const downloadUrl = trustedGitHubUrl(asset.browser_download_url);
    if (!name || size === null || !downloadUrl || !isReleaseAssetUrl(downloadUrl, tag)) return [];

    return [{ name, size, downloadUrl, type: classifyAsset(name) }];
  });
}

function getHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    'User-Agent': 'BarePDF-Website-Build',
    'Accept': 'application/vnd.github.v3+json',
  };
  
  const token = import.meta.env.GITHUB_TOKEN;
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
      console.warn(`[GitHub API] Failed to fetch latest release (${res.status} ${res.statusText}). Using static fallback.`);
      return defaultReleaseFallback;
    }
    const data: unknown = await res.json();
    if (!isRecord(data)) return defaultReleaseFallback;

    const tag = stringValue(data.tag_name);
    const releaseUrl = trustedGitHubUrl(data.html_url);
    if (!tag || !releaseUrl || !isReleasePageUrl(releaseUrl, tag)) return defaultReleaseFallback;
    const assets = parseReleaseAssets(data.assets, tag);
    if (assets.length === 0) return defaultReleaseFallback;

    return {
      tag,
      name: stringValue(data.name) ?? tag,
      publishedAt: stringValue(data.published_at),
      url: releaseUrl,
      notes: stringValue(data.body) ?? 'No release notes provided.',
      assets,
    };
  } catch (err) {
    console.warn(`[GitHub API] Error fetching latest release:`, err);
    return defaultReleaseFallback;
  }
}

export async function getRecentCommits(limit = 30): Promise<GitHubCommit[]> {
  const url = `${GITHUB_API_BASE}/repos/${repository.owner}/${repository.name}/commits?per_page=${limit}`;
  try {
    const res = await fetchWithTimeout(url);
    if (!res.ok) {
      console.warn(`[GitHub API] Failed to fetch commits (${res.status}). Returning static commits.`);
      return getFallbackCommits();
    }
    const data = await res.json();
    if (!Array.isArray(data)) {
      return getFallbackCommits();
    }

    const commits = data.flatMap((commit) => {
      if (!isRecord(commit) || !isRecord(commit.commit)) return [];
      const fullMsg = stringValue(commit.commit.message);
      const sha = stringValue(commit.sha);
      const url = trustedGitHubUrl(commit.html_url);
      if (!fullMsg || !sha || !/^[a-f0-9]{7,64}$/i.test(sha) || !url) return [];
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
  return [
    {
      sha: 'b3a4102089a8a5ec5351c43afe8be68d0a599f80',
      shortSha: 'b3a4102',
      message: 'feat: ship premium UI and stabilize PDF rendering',
      subject: 'feat: ship premium UI and stabilize PDF rendering',
      body: null,
      authorName: 'BarePDF Team',
      date: '2026-08-10T01:35:47+03:00',
      url: `https://github.com/${repository.owner}/${repository.name}`,
    },
  ];
}
