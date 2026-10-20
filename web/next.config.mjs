import nextra from 'nextra'

const withNextra = nextra({
  // Every code block gets a copy button. The engine's diagnostics and run
  // reports are meant to be pasted into an issue.
  defaultShowCopyCode: true,
  // Shiki, with one theme per colour scheme rather than a single theme
  // forced through a filter.
  mdxOptions: {
    rehypePrettyCodeOptions: {
      theme: {
        light: 'github-light',
        dark: 'github-dark-default',
      },
    },
  },
})

export default withNextra({
  // GitHub Pages serves a directory of files, not a Node server.
  output: 'export',
  // Pages has no image optimiser.
  images: { unoptimized: true },
  // Without this, `/concepts/caching` is emitted as `concepts/caching.html`
  // and relative links between pages resolve one level too high.
  trailingSlash: true,
  // The custom domain is at the zone root (ligase.neoforgelabs.tech), so the
  // site is served from `/`. A basePath would be needed only for the
  // github.io/ligase subpath.
  basePath: '',
  reactStrictMode: true,
  eslint: { ignoreDuringBuilds: true },
})
