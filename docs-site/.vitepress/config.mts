import { defineConfig } from 'vitepress'

const stable = '/0.5.0/'

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
        text: 'Version 0.5.0',
        items: [
          { text: '0.5.0 · Stable', link: stable },
          { text: 'Next · Unreleased', link: '/next/' },
          { text: 'All versions', link: '/versions' }
        ]
      },
      {
        text: 'Ecosystem',
        items: [
          { text: 'Crates.io', link: 'https://crates.io/crates/minco' },
          { text: 'Rust API docs', link: 'https://docs.rs/minco/0.5.0/minco/' },
          {
            text: 'Release notes',
            link: 'https://github.com/xicv/minco/releases/tag/v0.5.0'
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
      '/next/': [
        {
          text: 'Current Development',
          items: [
            { text: 'Overview', link: '/next/' },
            { text: 'Framework tour', link: '/next/getting-started/framework-tour' },
            { text: 'Project structure', link: '/next/getting-started/project-structure' }
          ]
        },
        {
          text: 'Guides',
          collapsed: false,
          items: [
            { text: 'Build a resource API', link: '/next/guides/resource-api' },
            {
              text: 'Test a plugin',
              link: '/next/guides/plugin-conformance'
            },
            { text: 'Plan an AWS deployment', link: '/next/guides/deployment' }
          ]
        },
        {
          text: 'Reference',
          collapsed: false,
          items: [
            { text: 'CLI', link: '/next/reference/cli' },
            { text: 'Resource API', link: '/next/reference/resource-api' },
            {
              text: 'Plugin conformance',
              link: '/next/reference/plugin-conformance'
            },
            { text: 'Testing and evidence', link: '/next/reference/testing' }
          ]
        },
        {
          text: 'Examples & Concepts',
          collapsed: false,
          items: [
            { text: 'Exercised examples', link: '/next/examples/' },
            { text: 'Zero idle, precisely', link: '/next/explanation/zero-idle' },
            { text: 'Stable 0.5.0', link: '/0.5.0/' }
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
