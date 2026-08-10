export interface ReleaseAsset {
  name: string;
  size: number;
  downloadUrl: string;
  type?: 'installer' | 'portable' | 'checksum' | 'other';
}

export function classifyAsset(filename: string): 'installer' | 'portable' | 'checksum' | 'other' {
  if (/Setup.*\.exe$/i.test(filename) || /Installer.*\.exe$/i.test(filename) || /\.exe$/i.test(filename)) {
    return 'installer';
  }
  if (/Portable.*\.zip$/i.test(filename) || /\.zip$/i.test(filename)) {
    return 'portable';
  }
  if (/SHA256SUMS/i.test(filename) || /checksum/i.test(filename) || /\.txt$/i.test(filename)) {
    return 'checksum';
  }
  return 'other';
}

export function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 Bytes';
  const k = 1024;
  const sizes = ['Bytes', 'KB', 'MB', 'GB'];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + ' ' + sizes[i];
}

export function formatDate(dateString: string | null): string {
  if (!dateString) return 'Recent';
  try {
    const d = new Date(dateString);
    return d.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  } catch {
    return dateString;
  }
}
