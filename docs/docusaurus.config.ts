import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'FORGE',
  tagline: 'What if PostgreSQL was enough?',
  favicon: 'img/favicon.svg',

  future: {
    v4: true,
  },

  url: 'https://tryforge.dev',
  baseUrl: '/',

  organizationName: 'isala404',
  projectName: 'forge',

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
          sidebarPath: './sidebars.ts',
          routeBasePath: '/docs',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
        sitemap: {
          changefreq: 'weekly',
          priority: 0.5,
          ignorePatterns: ['/tags/**'],
          filename: 'sitemap.xml',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    image: 'img/forge-social-card.jpg',
    colorMode: {
      defaultMode: 'dark',
      respectPrefersColorScheme: true,
    },
    navbar: {
      title: 'FORGE',
      logo: {
        alt: 'FORGE Logo',
        src: '/img/logo.png',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docsSidebar',
          position: 'left',
          label: 'Docs',
        },
        {
          href: 'https://github.com/isala404/forge',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      links: [
        {
          title: 'Start',
          items: [
            {
              label: 'Your First App',
              to: '/docs/start/first-app',
            },
            {
              label: 'Project Anatomy',
              to: '/docs/start/anatomy',
            },
          ],
        },
        {
          title: 'Reference',
          items: [
            {
              label: 'CLI',
              to: '/docs/reference/cli',
            },
            {
              label: 'Contexts',
              to: '/docs/reference/contexts',
            },
            {
              label: 'Attributes',
              to: '/docs/reference/attributes',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/isala404/forge',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} FORGE. MIT License.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'bash', 'typescript', 'sql', 'toml'],
    },
    algolia: {
      appId: 'I7YUFVJ0LL',
      apiKey: 'ce4a746b5b6be58663c139cf4f57eabb',
      indexName: 'Forge Docs',
      contextualSearch: true,
      searchPagePath: 'search',
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
