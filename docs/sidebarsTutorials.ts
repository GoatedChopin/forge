import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  tutorialsSidebar: [
    {
      type: 'doc',
      id: 'index',
      label: 'Overview',
    },
    {
      type: 'doc',
      id: 'build-a-todo-app',
      label: 'Build a Todo App',
    },
    {
      type: 'doc',
      id: 'user-authentication',
      label: 'User Authentication',
    },
    {
      type: 'doc',
      id: 'background-jobs',
      label: 'Background Jobs',
    },
    {
      type: 'doc',
      id: 'realtime-updates',
      label: 'Real-time Updates',
    },
    {
      type: 'doc',
      id: 'testing',
      label: 'Testing',
    },
  ],
};

export default sidebars;
