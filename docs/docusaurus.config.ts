import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'FORGE',
  tagline: 'From Schema to Ship in a Single Day',
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
        blog: {
          showReadingTime: true,
          blogTitle: 'FORGE Blog',
          blogDescription: 'Updates, tutorials, and insights from the FORGE team',
          routeBasePath: '/blog',
        },
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  plugins: [
    [
      '@docusaurus/plugin-content-docs',
      {
        id: 'tutorials',
        path: 'tutorials',
        routeBasePath: '/tutorials',
        sidebarPath: './sidebarsTutorials.ts',
      },
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
          to: '/tutorials',
          label: 'Tutorials',
          position: 'left',
        },
        {
          to: '/blog',
          label: 'Blog',
          position: 'left',
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
          title: 'Learn',
          items: [
            {
              label: 'Quick Start',
              to: '/docs/quick-start',
            },
            {
              label: 'Tutorials',
              to: '/tutorials',
            },
            {
              label: 'Core Concepts',
              to: '/docs/concepts/how-it-works',
            },
          ],
        },
        {
          title: 'Reference',
          items: [
            {
              label: 'API Reference',
              to: '/docs/api',
            },
            {
              label: 'CLI Reference',
              to: '/docs/cli',
            },
            {
              label: 'Blog',
              to: '/blog',
            },
          ],
        },
        {
          title: 'Community',
          items: [
            {
              label: 'Discord',
              href: 'https://discord.gg/forge',
            },
            {
              label: 'GitHub',
              href: 'https://github.com/isala404/forge',
            },
          ],
        },
      ],
      copyright: `Copyright © ${new Date().getFullYear()} FORGE. Built with Docusaurus.`,
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'bash', 'typescript', 'sql', 'toml'],
    },
    algolia: undefined,
  } satisfies Preset.ThemeConfig,
};

export default config;
