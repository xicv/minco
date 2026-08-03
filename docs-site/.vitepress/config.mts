import { defineConfig } from 'vitepress'
import release from '../release.json'

const stable = `/${release.stable}/`
const workspace = `/${release.workspace}/`
const candidateItem =
  release.state === 'candidate'
    ? [{ text: `${release.workspace} · Release candidate`, link: workspace }]
    : []

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
    ['meta', { name: 'theme-color', content: '#6d5ce7' }],
    ['link', { rel: 'icon', href: '/minco/minco-icon.svg', type: 'image/svg+xml' }]
  ],
  themeConfig: {
    logo: {
      src: '/minco-icon.svg',
      alt: 'Minco'
    },
    siteTitle: 'Minco',
    nav: [
      { text: 'Documentation', link: stable },
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
      '/next/': [
        {
          text: 'Start Here',
          collapsed: false,
          items: [
            { text: 'Overview', link: '/next/' },
            { text: 'Installation', link: '/next/getting-started/installation' },
            { text: 'Build your first application', link: '/next/getting-started/first-application' },
            { text: 'Framework tour', link: '/next/getting-started/framework-tour' },
            { text: 'Project structure', link: '/next/getting-started/project-structure' }
          ]
        },
        {
          text: 'Essentials',
          collapsed: false,
          items: [
            { text: 'Feature catalog', link: '/next/features/' },
            { text: 'Configuration', link: '/next/guides/configuration' },
            { text: 'Local development', link: '/next/guides/local-development' },
            { text: 'Build a resource API', link: '/next/guides/resource-api' },
            { text: 'Migrations and seeders', link: '/next/guides/database-lifecycle' },
            { text: 'Queues and workers', link: '/next/guides/background-work' },
            { text: 'Testing and evidence', link: '/next/reference/testing' }
          ]
        },
        {
          text: 'Application Services',
          collapsed: false,
          items: [
            { text: 'Identity and sessions', link: '/next/guides/identity-and-sessions' },
            { text: 'Files and static sites', link: '/next/guides/files-and-static-sites' },
            { text: 'Events and notifications', link: '/next/guides/events-and-notifications' },
            { text: 'Client feedback loop', link: '/next/guides/feedback' },
            { text: 'Resource API conventions', link: '/next/reference/resource-api' }
          ]
        },
        {
          text: 'Plugins and Extensions',
          collapsed: false,
          items: [
            { text: 'Built-in catalog', link: '/next/plugins/' },
            { text: 'Install and compose plugins', link: '/next/plugins/using-plugins' },
            {
              text: 'Test a plugin',
              link: '/next/guides/plugin-conformance'
            }
          ]
        },
        {
          text: 'Deploy and Operate',
          collapsed: false,
          items: [
            { text: 'Plan an AWS deployment', link: '/next/guides/deployment' },
            { text: 'Zero idle, precisely', link: '/next/explanation/zero-idle' },
            { text: 'Testing and evidence', link: '/next/reference/testing' }
          ]
        },
        {
          text: 'Cookbook',
          collapsed: false,
          items: [
            { text: 'Practical recipes', link: '/next/cookbook/' },
            { text: 'Orders API end to end', link: '/next/cookbook/orders-api' },
            { text: 'Exercised examples', link: '/next/examples/' }
          ]
        },
        {
          text: 'Reference',
          items: [
            { text: 'CLI commands', link: '/next/reference/cli' },
            { text: 'Cargo feature flags', link: '/next/reference/feature-flags' },
            {
              text: 'Plugin conformance',
              link: '/next/reference/plugin-conformance'
            },
            { text: `Stable ${release.stable}`, link: stable }
          ]
        }
      ]
    },
    search: {
      provider: 'local',
      options: {
        detailedView: true,
        translations: {
          button: {
            buttonText: 'Search Minco docs',
            buttonAriaLabel: 'Search Minco documentation'
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
      message: 'Minimal cost, maximum capability.',
      copyright: 'Released under the MIT License.'
    }
  }
})
