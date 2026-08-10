import { defineConfig } from 'astro/config';
import sitemap from '@astrojs/sitemap';

export default defineConfig({
  site: 'https://woffluon.github.io',
  base: '/BarePDF',
  integrations: [sitemap()],
  output: 'static',
  compressHTML: true,
  build: {
    format: 'directory'
  }
});
