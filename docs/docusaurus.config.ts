import type * as Preset from '@docusaurus/preset-classic';
import type {Config} from '@docusaurus/types';
import type {PrismTheme} from 'prism-react-renderer';

const repository = 'https://github.com/The1fEst/proscenio';

const fonts =
  'https://fonts.googleapis.com/css2?family=Google+Sans:ital,wght@0,400..700;1,400..700&family=JetBrains+Mono:ital,wght@0,400..700;1,400..700&display=swap';

type Palette = {
  surface: string;
  onSurface: string;
  outline: string;
  primary: string;
  secondary: string;
  tertiary: string;
  error: string;
  success: string;
};

function codeTheme(palette: Palette): PrismTheme {
  return {
    plain: {color: palette.onSurface, backgroundColor: palette.surface},
    styles: [
      {types: ['comment', 'prolog', 'doctype', 'cdata'], style: {color: palette.outline, fontStyle: 'italic'}},
      {types: ['punctuation', 'operator'], style: {color: palette.outline}},
      {types: ['keyword', 'selector', 'important', 'atrule'], style: {color: palette.primary}},
      {types: ['string', 'char', 'attr-value', 'regex', 'inserted'], style: {color: palette.success}},
      {types: ['number', 'boolean', 'constant', 'symbol'], style: {color: palette.tertiary}},
      {types: ['function', 'class-name', 'builtin', 'macro'], style: {color: palette.secondary, fontWeight: '500'}},
      {types: ['tag', 'property', 'attr-name', 'variable', 'key'], style: {color: palette.primary}},
      {types: ['deleted'], style: {color: palette.error}},
    ],
  };
}

const lightCode = codeTheme({
  surface: '#f2ecee',
  onSurface: '#1d1b1c',
  outline: '#777475',
  primary: '#675a68',
  secondary: '#645c64',
  tertiary: '#806f83',
  error: '#ba1a1a',
  success: '#4f6354',
});

const darkCode = codeTheme({
  surface: '#211f20',
  onSurface: '#e7e1e3',
  outline: '#949091',
  primary: '#d1c2d2',
  secondary: '#cec3cd',
  tertiary: '#d5c0d7',
  error: '#ffb4ab',
  success: '#b5ccba',
});

const config: Config = {
  title: 'proscenio',
  tagline: 'A desktop shell for Hyprland, written in Rust with GTK 4',
  favicon: 'img/proscenio-glyph.svg',
  stylesheets: [fonts],
  headTags: [
    {tagName: 'link', attributes: {rel: 'preconnect', href: 'https://fonts.googleapis.com'}},
    {
      tagName: 'link',
      attributes: {rel: 'preconnect', href: 'https://fonts.gstatic.com', crossorigin: 'anonymous'},
    },
  ],

  url: 'https://the1fest.github.io',
  baseUrl: '/proscenio/',
  organizationName: 'The1fEst',
  projectName: 'proscenio',
  trailingSlash: false,

  onBrokenLinks: 'throw',
  onBrokenAnchors: 'throw',
  onDuplicateRoutes: 'throw',

  future: {
    faster: true,
    v4: {
      removeLegacyPostBuildHeadAttribute: true,
      useCssCascadeLayers: false,
    },
  },

  i18n: {
    defaultLocale: 'en',
    locales: ['en'],
  },

  markdown: {
    format: 'md',
    mermaid: true,
    hooks: {
      onBrokenMarkdownLinks: 'throw',
      onBrokenMarkdownImages: 'throw',
    },
  },

  themes: [
    '@docusaurus/theme-mermaid',
    [
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        indexBlog: false,
        docsRouteBasePath: '/docs',
        highlightSearchTermsOnTargetPage: true,
        searchResultLimits: 12,
        searchBarShortcutHint: false,
      },
    ],
  ],

  presets: [
    [
      'classic',
      {
        docs: {
          routeBasePath: '/docs',
          sidebarPath: './sidebars.ts',
          editUrl: `${repository}/tree/main/docs/`,
          showLastUpdateTime: true,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  themeConfig: {
    colorMode: {
      respectPrefersColorScheme: true,
    },
    docs: {
      sidebar: {
        hideable: true,
        autoCollapseCategories: false,
      },
    },
    navbar: {
      title: 'proscenio',
      logo: {
        alt: 'proscenio',
        src: 'img/proscenio-glyph.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docs',
          position: 'left',
          label: 'Documentation',
        },
        {
          href: repository,
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'light',
      links: [
        {
          title: 'Documentation',
          items: [
            {label: 'Overview', to: '/docs'},
            {label: 'Building and installing', to: '/docs/installing'},
            {label: 'The settings window', to: '/docs/settings'},
            {label: 'Architecture', to: '/docs/architecture'},
          ],
        },
        {
          title: 'Scripting',
          items: [
            {label: 'Command line', to: '/docs/command-line'},
            {label: 'IPC', to: '/docs/ipc'},
            {label: 'Global shortcuts', to: '/docs/shortcuts'},
          ],
        },
        {
          title: 'Repository',
          items: [
            {label: 'Source', href: repository},
            {label: 'Issues', href: `${repository}/issues`},
          ],
        },
      ],
      copyright: 'Built with Docusaurus.',
    },
    prism: {
      theme: lightCode,
      darkTheme: darkCode,
      additionalLanguages: ['rust', 'bash', 'toml', 'json', 'lua', 'ini', 'diff'],
    },
    mermaid: {
      theme: {light: 'neutral', dark: 'dark'},
    },
  } satisfies Preset.ThemeConfig,
};

export default config;
