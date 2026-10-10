import assert from 'node:assert/strict';
import test from 'node:test';
import {
  isCommitUrl,
  isReleaseAssetUrl,
  releaseAssetNames,
  releaseAssetType,
  trustedGitHubUrl,
} from './github-validation.ts';
import {
  MAX_API_RESPONSE_BYTES,
  VERIFIED_DOWNLOAD_BASELINE,
  getDownloadMetrics,
  getLatestRelease,
  readBoundedJson,
} from './github.ts';

const owner = 'Woffluon';
const repository = 'BarePDF';
const tag = 'v1.1.0';

test('accepts one exact versioned name for each public release asset', () => {
  const names = releaseAssetNames(tag);
  assert.ok(names);
  assert.equal(releaseAssetType(names.installer, tag), 'installer');
  assert.equal(releaseAssetType(names.portable, tag), 'portable');
  assert.equal(releaseAssetType(names.checksum, tag), 'checksum');
  assert.equal(releaseAssetType('BarePDF-Setup-x64.exe', tag), null);
  assert.equal(releaseAssetType('BarePDF-Portable-x64.zip', tag), null);
  assert.equal(releaseAssetType('BarePDF-SHA256SUMS.txt', tag), null);
  assert.equal(releaseAssetType('BarePDF-Setup-x64-v1.0.0.exe', tag), null);
  assert.equal(releaseAssetType('unrelated.exe', tag), null);
  assert.equal(releaseAssetType('unrelated.zip', tag), null);
});

test('requires exact GitHub release and commit URLs', () => {
  const installer = 'BarePDF-Setup-x64-v1.1.0.exe';
  const assetUrl = `https://github.com/${owner}/${repository}/releases/download/${tag}/${installer}`;
  const sha = '0123456789abcdef0123456789abcdef01234567';

  assert.equal(isReleaseAssetUrl(assetUrl, owner, repository, tag, installer), true);
  assert.equal(isReleaseAssetUrl(`${assetUrl}?download=1`, owner, repository, tag, installer), false);
  assert.equal(isReleaseAssetUrl(assetUrl.replace(tag, 'v1.0.0'), owner, repository, tag, installer), false);
  assert.equal(isCommitUrl(`https://github.com/${owner}/${repository}/commit/${sha}`, owner, repository, sha), true);
  assert.equal(isCommitUrl(`https://github.com/${owner}/${repository}/commit/${sha}#diff`, owner, repository, sha), false);
});

test('rejects credentials, non-default ports, query strings, and fragments', () => {
  assert.equal(trustedGitHubUrl('https://github.com/Woffluon/BarePDF'), 'https://github.com/Woffluon/BarePDF');
  assert.equal(trustedGitHubUrl('https://user@github.com/Woffluon/BarePDF'), null);
  assert.equal(trustedGitHubUrl('https://github.com:444/Woffluon/BarePDF'), null);
  assert.equal(trustedGitHubUrl('https://github.com/Woffluon/BarePDF?tab=readme'), null);
  assert.equal(trustedGitHubUrl('https://github.com/Woffluon/BarePDF#readme'), null);
});

test('accepts only official GitHub release hosts and BarePDF repository paths', () => {
  assert.equal(
    trustedGitHubUrl('https://github.com/Woffluon/BarePDF/releases/tag/v1.1.0'),
    'https://github.com/Woffluon/BarePDF/releases/tag/v1.1.0',
  );
  assert.equal(
    trustedGitHubUrl('https://objects.githubusercontent.com/github-production-release-asset-2e65be/123/BarePDF-Setup-x64-v1.1.0.exe'),
    'https://objects.githubusercontent.com/github-production-release-asset-2e65be/123/BarePDF-Setup-x64-v1.1.0.exe',
  );
  assert.equal(
    trustedGitHubUrl('https://release-assets.githubusercontent.com/github-production-release-asset/123/BarePDF-Setup-x64-v1.1.0.exe'),
    'https://release-assets.githubusercontent.com/github-production-release-asset/123/BarePDF-Setup-x64-v1.1.0.exe',
  );
  assert.equal(
    trustedGitHubUrl('https://raw.githubusercontent.com/attacker/repo/main/BarePDF-Setup-x64-v1.2.3.exe'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://raw.githubusercontent.com/Woffluon/BarePDF/main/README.md'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://gist.githubusercontent.com/attacker/123/raw/payload.exe'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://camo.githubusercontent.com/123456'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://avatars.githubusercontent.com/u/123456'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://github.com/Woffluon/OtherRepo/releases/tag/v1.1.0'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://github.com/Woffluon/BarePDF-evil/releases/tag/v1.1.0'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://github.com/attacker/BarePDF/releases/tag/v1.1.0'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://release-assets.githubusercontent.com/unapproved/file'),
    null,
  );
  assert.equal(
    trustedGitHubUrl('https://objects.githubusercontent.com/unapproved/file'),
    null,
  );
});

test('rejects oversized GitHub API responses before buffering or parsing', async () => {
  assert.equal(MAX_API_RESPONSE_BYTES, 2 * 1024 * 1024);

  const validResponse = new Response(JSON.stringify({ ok: true }), {
    status: 200,
    headers: { 'content-type': 'application/json' },
  });
  assert.deepEqual(await readBoundedJson(validResponse, 1024), { ok: true });

  let bodyPulled = false;
  const oversizedHeaderStream = new ReadableStream<Uint8Array>({
    pull(controller) {
      bodyPulled = true;
      controller.enqueue(new TextEncoder().encode('{"ok":true}'));
      controller.close();
    },
  });
  const oversizedHeaderResponse = new Response(oversizedHeaderStream, {
    status: 200,
    headers: { 'content-length': String(MAX_API_RESPONSE_BYTES + 1) },
  });

  await assert.rejects(
    () => readBoundedJson(oversizedHeaderResponse),
    /exceeds/i,
  );
  assert.equal(bodyPulled, false);

  let chunksRead = 0;
  const oversizedChunkStream = new ReadableStream<Uint8Array>({
    pull(controller) {
      chunksRead += 1;
      controller.enqueue(new Uint8Array(600));
      if (chunksRead >= 5) {
        controller.close();
      }
    },
  });
  const oversizedChunkResponse = new Response(oversizedChunkStream, { status: 200 });

  await assert.rejects(
    () => readBoundedJson(oversizedChunkResponse, 1024),
    /exceeds/i,
  );
  assert.equal(chunksRead, 2);

  const originalFetch = globalThis.fetch;
  try {
    globalThis.fetch = async () =>
      new Response(JSON.stringify({ tag_name: 'v1.1.0' }), {
        status: 200,
        headers: { 'content-length': String(MAX_API_RESPONSE_BYTES + 100) },
      });
    const fallback = await getLatestRelease();
    assert.equal(fallback.state, 'fallback');
  } finally {
    globalThis.fetch = originalFetch;
  }
});

test('download metrics parses releases and falls back safely', async () => {
  assert.equal(VERIFIED_DOWNLOAD_BASELINE.totalDownloads, 149);
  assert.equal(VERIFIED_DOWNLOAD_BASELINE.installerDownloads, 104);
  assert.equal(VERIFIED_DOWNLOAD_BASELINE.portableDownloads, 45);

  const originalFetch = globalThis.fetch;
  try {
    // Failure falls back to verified baseline
    globalThis.fetch = async () => new Response('Internal error', { status: 500 });
    const metricsFallback = await getDownloadMetrics();
    assert.equal(metricsFallback.isFallback, true);
    assert.equal(metricsFallback.totalDownloads, 149);

    // Mocked releases response parses asset download counts
    globalThis.fetch = async () =>
      new Response(
        JSON.stringify([
          {
            draft: false,
            prerelease: false,
            assets: [
              { name: 'BarePDF-Setup-x64-v1.17.1.exe', download_count: 150 },
              { name: 'BarePDF-Portable-x64-v1.17.1.zip', download_count: 50 },
            ],
          },
        ]),
        { status: 200 },
      );
    const parsed = await getDownloadMetrics();
    assert.equal(parsed.isFallback, false);
    assert.equal(parsed.installerDownloads, 150);
    assert.equal(parsed.portableDownloads, 50);
    assert.equal(parsed.totalDownloads, 200);
    assert.equal(parsed.latestReleaseDownloads, 200);
  } finally {
    globalThis.fetch = originalFetch;
  }
});
