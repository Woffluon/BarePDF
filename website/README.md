# BarePDF Website

This directory contains the official open-source website and documentation for **BarePDF**, built with **Astro 5+**, **TypeScript**, and **Scoped CSS / CSS Tokens**.

## Features

- **Static Site Generation (SSG)**: Fast rendering, zero server backend required.
- **GitHub Integration**: Automatically fetches latest releases, assets, and commit activity at build time with graceful fallback.
- **Content Collections**: Markdown documentation for User and Developer guides.
- **Zero Framework Overhead**: Vanilla JS micro-scripts for dark/light theme switching and mobile navigation.
- **Accessibility**: WCAG 2.2 AA compliance, skip navigation, full keyboard accessibility, and `prefers-reduced-motion` support.

## Local Development

```bash
# Navigate to website directory
cd website

# Install dependencies
pnpm install

# Start local development server
pnpm dev

# Type check Astro & TypeScript
pnpm astro check

# Build production static output
pnpm build

# Preview production build locally
pnpm preview
```

## Structure

```text
website/
├── astro.config.mjs
├── package.json
├── tsconfig.json
├── public/
│   ├── favicon.ico
│   ├── favicon.svg
│   ├── robots.txt
│   ├── site.webmanifest
│   └── images/
│
└── src/
    ├── components/
    │   ├── Header.astro
    │   ├── Footer.astro
    │   ├── Hero.astro
    │   ├── AppScreenshot.astro
    │   ├── FeatureGrid.astro
    │   ├── Philosophy.astro
    │   ├── DownloadCard.astro
    │   ├── ReleaseCard.astro
    │   ├── CommitList.astro
    │   ├── DocsSidebar.astro
    │   └── ThemeToggle.astro
    │
    ├── content/
    │   ├── config.ts
    │   └── docs/
    │       ├── user/
    │       └── developer/
    │
    ├── layouts/
    │   ├── BaseLayout.astro
    │   └── DocsLayout.astro
    │
    ├── lib/
    │   ├── github.ts
    │   ├── releases.ts
    │   └── repository.ts
    │
    ├── pages/
    │   ├── index.astro
    │   ├── download.astro
    │   ├── changelog.astro
    │   ├── docs/
    │   └── 404.astro
    │
    └── styles/
        ├── tokens.css
        └── global.css
```

## GitHub API Integration & Environment Variables

Build-time fetching uses public GitHub REST API endpoints.

To avoid rate limits during CI/CD builds, set:

```env
GITHUB_TOKEN=your_github_token_here
```

If the GitHub API is unavailable or rate-limited, the build automatically falls back to static release metadata defined in `src/lib/repository.ts`.
