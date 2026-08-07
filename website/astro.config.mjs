import { defineConfig } from 'astro/config';
import mdx from '@astrojs/mdx';

export default defineConfig({
  site: 'https://woffluon.github.io',
  base: '/BarePDF',
  integrations: [mdx()],
  output: 'static',
  compressHTML: true,
  build: {
    format: 'directory'
  }
});
