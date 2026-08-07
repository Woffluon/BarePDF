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

export function cleanReleaseNotes(notes: string | null | undefined): string {
  if (!notes) return 'Official release of BarePDF.';

  // 1. Remove HTML comments <!-- ... -->
  let cleaned = notes.replace(/<!--[\s\S]*?-->/g, '').trim();

  // 2. Process markdown links [text](url) -> <a href="url" target="_blank" rel="noopener noreferrer">text</a>
  cleaned = cleaned.replace(/\[([^\]]+)\]\(([^)]+)\)/g, '<a href="$2" target="_blank" rel="noopener noreferrer">$1</a>');

  // 3. Process markdown bold **text** -> <strong>text</strong>
  cleaned = cleaned.replace(/\*\*([^*]+)\*\*/g, '<strong>$1</strong>');

  // 4. Process raw URLs that are not already in href="..."
  cleaned = cleaned.replace(/(?<!href=")(https?:\/\/[^\s<]+)/g, '<a href="$1" target="_blank" rel="noopener noreferrer">$1</a>');

  // 5. Clean up redundant empty lines
  const lines = cleaned.split('\n').map(l => l.trim()).filter(Boolean);
  const uniqueLines = Array.from(new Set(lines));

  if (uniqueLines.length === 0) {
    return 'Official production release of BarePDF.';
  }

  return uniqueLines.join('<br />');
}
