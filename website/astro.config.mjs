import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';
import sitemap from '@astrojs/sitemap';

export default defineConfig({
  site: 'https://woffluon.github.io',
  base: '/BarePDF',
  integrations: [mdx(), sitemap()],
  output: 'static',
  compressHTML: true,
  build: {
    format: 'directory'
  }
});
