import type {SidebarsConfig} from '@docusaurus/plugin-content-docs';

const sidebars: SidebarsConfig = {
  docs: [
    'index',
    {
      type: 'category',
      label: 'Start here',
      collapsed: false,
      items: ['installing', 'first-run', 'files', 'command-line'],
    },
    {
      type: 'category',
      label: 'The shell',
      items: [
        'bar',
        'workspaces',
        'tray',
        'sidebar',
        'quick-toggles',
        'notifications',
        'dock',
        'overview',
        'background',
        'screen-corners',
        'calendar',
        'media-controls',
        'osd',
        'on-screen-keyboard',
        'cheatsheet',
        'region-selector',
        'wallpaper-selector',
        'session-screen',
        'lock',
      ],
    },
    {
      type: 'category',
      label: 'Settings',
      link: {type: 'doc', id: 'settings'},
      items: [
        'settings-network',
        'settings-devices',
        'settings-displays',
        'settings-sound',
        'settings-power',
        'settings-multitasking',
        'settings-appearance',
        'settings-apps',
        'settings-input',
        'settings-system',
      ],
    },
    {
      type: 'category',
      label: 'Look and feel',
      items: ['colors', 'design'],
    },
    {
      type: 'category',
      label: 'Integration',
      items: ['shortcuts', 'ipc', 'polkit', 'keyring-prompt'],
    },
    {
      type: 'category',
      label: 'Internals',
      items: ['architecture', 'hyprland'],
    },
  ],
};

export default sidebars;
