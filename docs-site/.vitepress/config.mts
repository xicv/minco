import { defineConfig } from 'vitepress'
import release from '../release.json'

const stable = `/${release.stable}/`
const workspace = `/${release.workspace}/`
const candidateItem =
  release.state === 'candidate'
    ? [{ text: `${release.workspace} · Release candidate`, link: workspace }]
    : []

function workspaceSidebar(root: string) {
  const link = (path = '') => `${root}${path}`
  return [
    {
      text: 'Start Here',
      collapsed: false,
      items: [
        { text: 'Overview', link: link() },
        { text: 'Find a page', link: link('reference/documentation-map') },
        { text: 'Installation', link: link('getting-started/installation') },
        { text: 'Build your first application', link: link('getting-started/first-application') },
        { text: 'Framework tour', link: link('getting-started/framework-tour') },
        { text: 'Project structure', link: link('getting-started/project-structure') },
        { text: 'Architecture', link: link('explanation/architecture') }
      ]
    },
    {
      text: 'Essentials',
      collapsed: false,
      items: [
        { text: 'Feature catalog', link: link('features/') },
        { text: 'Configuration', link: link('guides/configuration') },
        { text: 'Local development', link: link('guides/local-development') },
        { text: 'Troubleshooting', link: link('guides/troubleshooting') },
        { text: 'Develop with Codex and Claude', link: link('guides/agent-development') },
        { text: 'Project view, MCP, and workbench', link: link('guides/project-view') },
        { text: 'Build a resource API', link: link('guides/resource-api') },
        ...(root === '/next/' || root === workspace
          ? [{ text: 'Browser and native clients', link: link('guides/mobile-api') }]
          : []),
        { text: 'Migrations and seeders', link: link('guides/database-lifecycle') },
        { text: 'Queues and workers', link: link('guides/background-work') },
        { text: 'Testing and evidence', link: link('reference/testing') }
      ]
    },
    {
      text: 'Application Services',
      collapsed: false,
      items: [
        { text: 'Identity and sessions', link: link('guides/identity-and-sessions') },
        { text: 'Files and static sites', link: link('guides/files-and-static-sites') },
        { text: 'Events and notifications', link: link('guides/events-and-notifications') },
        { text: 'Durable action auditing', link: link('guides/auditing') },
        { text: 'Realtime subscriptions', link: link('guides/realtime') },
        { text: 'Client feedback loop', link: link('guides/feedback') },
        { text: 'Ticketing support entry', link: link('guides/ticketing') },
        { text: 'Waffo hosted payments', link: link('guides/payments-waffo') },
        { text: 'Resource API conventions', link: link('reference/resource-api') }
      ]
    },
    {
      text: 'Plugins and Extensions',
      collapsed: false,
      items: [
        { text: 'Built-in catalog', link: link('plugins/') },
        { text: 'Install and compose plugins', link: link('plugins/using-plugins') },
        { text: 'Test a plugin', link: link('guides/plugin-conformance') }
      ]
    },
    {
      text: 'Deploy and Operate',
      collapsed: false,
      items: [
        { text: 'Plan an AWS deployment', link: link('guides/deployment') },
        { text: 'Protect traffic at the gateway', link: link('guides/traffic-policy') },
        { text: 'Compress HTTP delivery', link: link('guides/http-compression') },
        { text: 'Use the DynamoDB adapter', link: link('guides/dynamodb') },
        { text: 'Zero idle, precisely', link: link('explanation/zero-idle') },
        { text: 'Production blueprint', link: link('cookbook/production-blueprint') },
        { text: 'Testing and evidence', link: link('reference/testing') }
      ]
    },
    {
      text: 'Cookbook',
      collapsed: false,
      items: [
        { text: 'Practical recipes', link: link('cookbook/') },
        { text: 'Production blueprint', link: link('cookbook/production-blueprint') },
        { text: 'Orders API end to end', link: link('cookbook/orders-api') },
        { text: 'Exercised examples', link: link('examples/') }
      ]
    },
    {
      text: 'Reference',
      items: [
        { text: 'Find a page', link: link('reference/documentation-map') },
        { text: 'CLI commands', link: link('reference/cli') },
        { text: 'Cargo feature flags', link: link('reference/feature-flags') },
        { text: 'Resource API', link: link('reference/resource-api') },
        { text: 'Plugin conformance', link: link('reference/plugin-conformance') },
        { text: 'Testing and evidence', link: link('reference/testing') },
        { text: `Stable ${release.stable}`, link: stable }
      ]
    }
  ]
}

export default defineConfig({
  lang: 'en-US',
  title: 'Minco',
  titleTemplate: ':title · Minco',
  description:
    'Contract-to-cloud Rust framework for low-idle-cost web applications on AWS.',
  base: '/minco/',
  cleanUrls: true,
  lastUpdated: true,
  ignoreDeadLinks: false,
  sitemap: {
    hostname: 'https://xicv.github.io/minco/'
  },
  transformHead({ pageData }) {
    const path = pageData.relativePath
      .replace(/(^|\/)index\.md$/, '$1')
      .replace(/\.md$/, '')
    return [
      [
        'link',
        {
          rel: 'canonical',
          href: `https://xicv.github.io/minco/${path}`
        }
      ]
    ]
  },
  head: [
    ['meta', { name: 'theme-color', content: '#10151d' }],
    ['meta', { name: 'color-scheme', content: 'light dark' }],
    ['link', { rel: 'icon', href: '/minco/minco-icon.svg', type: 'image/svg+xml' }]
  ],
  themeConfig: {
    logo: {
      src: '/minco-icon.svg',
      alt: 'Minco connected runtime mark'
    },
    siteTitle: 'Minco',
    nav: [
      { text: 'Documentation', link: stable },
      { text: 'Find a page', link: `${stable}reference/documentation-map` },
      { text: 'Blueprint', link: `${stable}cookbook/production-blueprint` },
      {
        text: `Version ${release.stable}`,
        items: [
          { text: `${release.stable} · Stable`, link: stable },
          ...candidateItem,
          { text: 'Next · Unreleased', link: '/next/' },
          { text: 'All versions', link: '/versions' }
        ]
      },
      {
        text: 'Ecosystem',
        items: [
          { text: 'Crates.io', link: 'https://crates.io/crates/minco' },
          {
            text: 'Rust API docs',
            link: `https://docs.rs/minco/${release.stable}/minco/`
          },
          {
            text: 'Release notes',
            link: `https://github.com/xicv/minco/releases/tag/v${release.stable}`
          }
        ]
      }
    ],
    sidebar: {
      '/0.5.0/': [
        {
          text: 'Minco 0.5.0',
          items: [
            { text: 'Introduction', link: '/0.5.0/' },
            { text: 'Installation', link: '/0.5.0/installation' }
          ]
        },
        {
          text: 'Tutorials',
          collapsed: false,
          items: [
            { text: 'Build your first API', link: '/0.5.0/tutorials/first-api' },
            { text: 'Deploy to AWS', link: '/0.5.0/tutorials/deploy-to-aws' },
            { text: 'Build a plugin', link: '/0.5.0/tutorials/build-a-plugin' }
          ]
        },
        {
          text: 'How-to guides',
          items: [
            { text: 'Build a resource API', link: '/0.5.0/how-to/resource-api' },
            {
              text: 'Configure environments',
              link: '/0.5.0/how-to/configure-environments'
            },
            { text: 'Plan a deployment', link: '/0.5.0/how-to/plan-deployment' }
          ]
        },
        {
          text: 'Reference',
          items: [
            { text: 'CLI', link: '/0.5.0/reference/cli' },
            { text: 'Resource API', link: '/0.5.0/reference/resource-api' },
            { text: 'Testing', link: '/0.5.0/reference/testing' }
          ]
        },
        {
          text: 'Concepts',
          items: [
            {
              text: 'Contract-to-cloud architecture',
              link: '/0.5.0/explanation/architecture'
            },
            {
              text: 'Zero idle, precisely',
              link: '/0.5.0/explanation/zero-idle'
            }
          ]
        }
      ],
      '/0.6.0/': [
        {
          text: 'Minco 0.6.0',
          items: [
            { text: 'Introduction', link: '/0.6.0/' },
            { text: 'Installation', link: '/0.6.0/installation' },
            { text: 'Framework tour', link: '/0.6.0/getting-started/framework-tour' },
            { text: 'Project structure', link: '/0.6.0/getting-started/project-structure' }
          ]
        },
        {
          text: 'Tutorials',
          collapsed: false,
          items: [
            { text: 'Build your first API', link: '/0.6.0/tutorials/first-api' },
            { text: 'Deploy to AWS', link: '/0.6.0/tutorials/deploy-to-aws' },
            { text: 'Build a plugin', link: '/0.6.0/tutorials/build-a-plugin' }
          ]
        },
        {
          text: 'Guides',
          collapsed: false,
          items: [
            { text: 'Build a resource API', link: '/0.6.0/guides/resource-api' },
            { text: 'Test a plugin', link: '/0.6.0/guides/plugin-conformance' },
            { text: 'Plan an AWS deployment', link: '/0.6.0/guides/deployment' },
            { text: 'Configure environments', link: '/0.6.0/how-to/configure-environments' },
            { text: 'Review a deployment plan', link: '/0.6.0/how-to/plan-deployment' }
          ]
        },
        {
          text: 'Reference',
          collapsed: false,
          items: [
            { text: 'CLI', link: '/0.6.0/reference/cli' },
            { text: 'Resource API', link: '/0.6.0/reference/resource-api' },
            { text: 'Plugin distribution', link: '/0.6.0/reference/plugin-distribution' },
            { text: 'Plugin conformance', link: '/0.6.0/reference/plugin-conformance' },
            { text: 'Testing and evidence', link: '/0.6.0/reference/testing' }
          ]
        },
        {
          text: 'Examples & Concepts',
          collapsed: false,
          items: [
            { text: 'Exercised examples', link: '/0.6.0/examples/' },
            { text: 'Architecture', link: '/0.6.0/explanation/architecture' },
            { text: 'Zero idle, precisely', link: '/0.6.0/explanation/zero-idle' }
          ]
        }
      ],
      '/next/': workspaceSidebar('/next/'),
      [workspace]: workspaceSidebar(workspace)
    },
    search: {
      provider: 'local',
      options: {
        detailedView: true,
        _render(src, env, md) {
          const html = md.render(src, env)
          if (env.frontmatter?.search === false) return ''

          const path = env.relativePath.replaceAll('\\', '/')
          const isVersionedManual = /^\d+\.\d+\.\d+\//.test(path)
          const isInactiveVersionedManual =
            isVersionedManual &&
            (release.state === 'candidate' || !path.startsWith(`${release.stable}/`))
          const isPublishedNextDuplicate =
            release.state === 'published' && path.startsWith('next/')

          return isInactiveVersionedManual || isPublishedNextDuplicate
            ? ''
            : html
        },
        miniSearch: {
          searchOptions: {
            fuzzy: 0.2,
            prefix: true,
            boost: { title: 5, text: 2, titles: 2 }
          }
        },
        translations: {
          button: {
            buttonText: 'Search Minco docs',
            buttonAriaLabel: 'Search Minco documentation'
          },
          modal: {
            noResultsText: 'No current Minco documentation found for this query.'
          }
        }
      }
    },
    outline: {
      level: [2, 3],
      label: 'On this page'
    },
    editLink: {
      pattern: 'https://github.com/xicv/minco/edit/main/docs-site/:path',
      text: 'Improve this page'
    },
    socialLinks: [{ icon: 'github', link: 'https://github.com/xicv/minco' }],
    footer: {
      message: 'Contract. Plan. Run. Prove.',
      copyright: 'Released under the MIT License.'
    }
  }
})
