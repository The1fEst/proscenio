import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'index',
    {
      type: 'category',
      label: 'Start here',
      collapsed: false,
      items: ['installing'],
    },
  ],
};

export default sidebars;
