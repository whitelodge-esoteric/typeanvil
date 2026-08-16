import {themes as prismThemes} from 'prism-react-renderer';
import type {Config} from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

// This runs in Node.js - Don't use client-side code here (browser APIs, JSX...)

const config: Config = {
  title: 'Typeanvil Docs',
  tagline: 'HTML/Markdown → PDF typesetting engine — engineering source of truth',

  // Future flags, see https://docusaurus.io/docs/api/docusaurus-config#future
  // v4 flag left off: it reworks markdown/mermaid config; verify compatibility
  // with @docusaurus/theme-mermaid before enabling.

  // Production URL — placeholder until Typeanvil has a domain.
  url: 'https://typeanvil.dev',
  baseUrl: '/',

  onBrokenLinks: 'throw',

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          // Serve the repo's docs/ directory (the single source of truth) as
          // the whole site. Docs live at /docs/... paths relative to repo root.
          path: '../docs',
          routeBasePath: '/',
          // Auto-generate the sidebar from the folder structure + _category_.yml.
          sidebarPath: false,
        },
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  // Mermaid diagrams in docs/ render natively.
  markdown: {
    mermaid: true,
  },
  themes: ['@docusaurus/theme-mermaid'],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'Typeanvil',
      items: [],
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
