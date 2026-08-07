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

function getHeaders(): Record<string, string> {
  const headers: Record<string, string> = {
    'User-Agent': 'BarePDF-Website-Build',
    'Accept': 'application/vnd.github.v3+json',
  };
  
  // Use GITHUB_TOKEN environment variable if available during build
  const token = import.meta.env.GITHUB_TOKEN || (typeof globalThis !== 'undefined' && (globalThis as any).process?.env?.GITHUB_TOKEN);
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
    const data = await res.json();
    
    const assets: ReleaseAsset[] = (data.assets || []).map((a: any) => ({
      name: a.name || 'unnamed',
      size: a.size || 0,
      downloadUrl: a.browser_download_url || '',
      type: classifyAsset(a.name || ''),
    }));

    return {
      tag: data.tag_name || 'v1.0.0',
      name: data.name || data.tag_name || 'BarePDF Release',
      publishedAt: data.published_at || null,
      url: data.html_url || `https://github.com/${repository.owner}/${repository.name}/releases`,
      notes: data.body || 'No release notes provided.',
      assets: assets.length > 0 ? assets : defaultReleaseFallback.assets,
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

    return data.map((c: any) => {
      const fullMsg = c.commit?.message || '';
      const lines = fullMsg.split('\n');
      const subject = lines[0] || 'No commit message';
      const body = lines.slice(1).join('\n').trim() || null;
      const sha = c.sha || 'unknown';

      return {
        sha,
        shortSha: sha.substring(0, 7),
        message: fullMsg,
        subject,
        body,
        authorName: c.commit?.author?.name || c.author?.login || 'Contributor',
        date: c.commit?.author?.date || null,
        url: c.html_url || `https://github.com/${repository.owner}/${repository.name}/commit/${sha}`,
      };
    });
  } catch (err) {
    console.warn(`[GitHub API] Error fetching commits:`, err);
    return getFallbackCommits();
  }
}

function getFallbackCommits(): GitHubCommit[] {
  return [
    {
      sha: 'a1b2c3d4e5f6',
      shortSha: 'a1b2c3d',
      message: 'feat(website): add Astro site, documentation, releases, and changelog',
      subject: 'feat(website): add Astro site, documentation, releases, and changelog',
      body: null,
      authorName: 'BarePDF Team',
      date: '2026-08-07T18:00:00Z',
      url: `https://github.com/${repository.owner}/${repository.name}`,
    },
    {
      sha: 'f7e6d5c4b3a2',
      shortSha: 'f7e6d5c',
      message: 'ci: add release packaging pipeline and checksum generation',
      subject: 'ci: add release packaging pipeline and checksum generation',
      body: null,
      authorName: 'BarePDF Team',
      date: '2026-08-07T15:00:00Z',
      url: `https://github.com/${repository.owner}/${repository.name}`,
    },
    {
      sha: '9876543210ab',
      shortSha: '9876543',
      message: 'feat(ui): implement continuous vertical reading and Slint dark theme',
      subject: 'feat(ui): implement continuous vertical reading and Slint dark theme',
      body: null,
      authorName: 'BarePDF Team',
      date: '2026-08-07T12:00:00Z',
      url: `https://github.com/${repository.owner}/${repository.name}`,
    },
  ];
}
