import assert from 'node:assert/strict';
import test from 'node:test';
import {
  isCommitUrl,
  isReleaseAssetUrl,
  releaseAssetNames,
  releaseAssetType,
  trustedGitHubUrl,
} from './github-validation.ts';

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
