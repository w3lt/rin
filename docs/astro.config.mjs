// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// Served by GitHub Pages at the root of its custom domain (set in the repository's
// Pages settings), so links between pages start at `/` (`/guides/team/`).
export default defineConfig({
	site: 'https://rin.nent.io',
	integrations: [
		starlight({
			title: 'Rin',
			description:
				'Run a team of AI agents on your own machine: one config file, one Docker container per agent.',
			logo: { src: './src/assets/logo.svg', replacesTitle: false },
			favicon: '/favicon.svg',
			customCss: ['./src/styles/rin.css'],
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/w3lt/rin' }],
			editLink: { baseUrl: 'https://github.com/w3lt/rin/edit/main/docs/' },
			lastUpdated: true,
			sidebar: [
				{
					label: 'Start here',
					items: [
						{ label: 'What is Rin?', slug: 'start/introduction' },
						{ label: 'Installation', slug: 'start/installation' },
						{ label: 'Quickstart', slug: 'start/quickstart' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Design your team', slug: 'guides/team' },
						{ label: 'Subscriptions and API keys', slug: 'guides/authentication' },
						{ label: 'Talking to agents', slug: 'guides/messaging' },
						{ label: 'Running several teams', slug: 'guides/teams' },
						{ label: 'Changing and removing a team', slug: 'guides/lifecycle' },
						{ label: 'Troubleshooting', slug: 'guides/troubleshooting' },
					],
				},
				{
					label: 'Concepts',
					items: [{ label: 'How Rin works', slug: 'concepts/architecture' }],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'CLI', slug: 'reference/cli' },
						{ label: 'rin.yml', slug: 'reference/config' },
						{ label: 'Harnesses', slug: 'reference/harnesses' },
						{ label: 'Files and environment', slug: 'reference/files' },
					],
				},
			],
		}),
	],
});
