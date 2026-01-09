import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docsSidebar: [
    {
      type: 'doc',
      id: 'index',
      label: 'Introduction',
    },
    {
      type: 'doc',
      id: 'quick-start',
      label: 'Quick Start',
    },
    {
      type: 'doc',
      id: 'why-forge',
      label: 'Why FORGE?',
    },
    {
      type: 'category',
      label: 'Core Concepts',
      collapsed: false,
      items: [
        'concepts/how-it-works',
        'concepts/schema',
        'concepts/functions',
        'concepts/realtime',
      ],
    },
    {
      type: 'category',
      label: 'Background Processing',
      collapsed: true,
      items: [
        'background/index',
        'background/jobs',
        'background/crons',
        'background/workflows',
      ],
    },
    {
      type: 'category',
      label: 'Frontend',
      collapsed: true,
      items: [
        'frontend/index',
        'frontend/setup',
        'frontend/queries-mutations',
        'frontend/realtime-subscriptions',
        'frontend/job-tracking',
      ],
    },
    {
      type: 'category',
      label: 'API Reference',
      collapsed: true,
      items: [
        'api/index',
        'api/configuration',
        'api/query-context',
        'api/mutation-context',
        'api/action-context',
        'api/job-context',
        'api/workflow-context',
        'api/forge-error',
      ],
    },
    {
      type: 'doc',
      id: 'cli/index',
      label: 'CLI Reference',
    },
    {
      type: 'category',
      label: 'Advanced',
      collapsed: true,
      items: [
        'concepts/deployment',
        'concepts/observability',
        'concepts/cluster',
        'concepts/multi-tenancy',
        'concepts/rate-limiting',
      ],
    },
    {
      type: 'category',
      label: 'Compare',
      collapsed: true,
      items: [
        'compare/index',
        'compare/supabase',
        'compare/firebase',
        'compare/pocketbase',
        'compare/convex',
      ],
    },
    {
      type: 'category',
      label: 'Guides',
      collapsed: true,
      items: [
        'guides/index',
        'guides/troubleshooting',
        'guides/migrate-from-supabase',
        'guides/migrate-from-firebase',
        'guides/migrate-from-node',
      ],
    },
  ],
};

export default sidebars;
