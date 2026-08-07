export const repository = {
  owner: 'Woffluon',
  name: 'BarePDF',
  motto: 'Bare, Fast, Yours.',
  url: 'https://github.com/Woffluon/BarePDF',
  defaultBranch: 'main',
  license: 'MIT',
  version: '1.0.0',
  description: 'Bare, Fast, Yours. Lightweight open-source PDF reader for Windows built with Rust and PDFium.',
};

export const defaultReleaseFallback = {
  tag: 'v1.0.0',
  name: 'BarePDF v1.0.0',
  publishedAt: '2026-08-07T12:00:00Z',
  url: 'https://github.com/Woffluon/BarePDF/releases/tag/v1.0.0',
  notes: 'Initial release of BarePDF. Built for speed with PDFium rendering, Slint UI, continuous vertical reading, full screen, presentation mode, and Windows installer.',
  assets: [
    {
      name: 'BarePDF-Setup-x64-v1.0.0.exe',
      size: 19320832,
      downloadUrl: 'https://github.com/Woffluon/BarePDF/releases/download/v1.0.0/BarePDF-Setup-x64-v1.0.0.exe',
      type: 'installer' as const,
    },
    {
      name: 'BarePDF-Portable-x64-v1.0.0.zip',
      size: 18454912,
      downloadUrl: 'https://github.com/Woffluon/BarePDF/releases/download/v1.0.0/BarePDF-Portable-x64-v1.0.0.zip',
      type: 'portable' as const,
    },
    {
      name: 'BarePDF-v1.0.0-SHA256SUMS.txt',
      size: 456,
      downloadUrl: 'https://github.com/Woffluon/BarePDF/releases/download/v1.0.0/BarePDF-v1.0.0-SHA256SUMS.txt',
      type: 'checksum' as const,
    },
  ],
};
