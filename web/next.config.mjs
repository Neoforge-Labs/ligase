import nextra from 'nextra'

const withNextra = nextra({
  // Every code block gets a copy button. The engine's diagnostics and run
  // reports are meant to be pasted into an issue.
  defaultShowCopyCode: true,
  // Shiki, with one theme per colour scheme rather than a single theme
  // forced through a filter. The pair is the high-contrast GitHub one, chosen
  // by measuring every token colour against the surface a code block is
  // actually drawn on (white and #17171c; the stylesheet overrides the
  // theme's own background). The previous light theme, github-light, had
  // keywords at 3.8:1 and variables at 2.9:1 on white. Here every code token
  // clears 4.5:1, the closest being light-mode comments at 5.04:1 (dark: 8.42:1),
  // and `npm run shiki` re-measures that against the built HTML. The hues are
  // the restrained ones (deep red, blue, brown, green) that do not compete
  // with the flame orange the site spends on links and the mark. Bundled
  // Shiki themes only.
  mdxOptions: {
    rehypePrettyCodeOptions: {
      theme: {
        light: 'github-light-high-contrast',
        dark: 'github-dark-high-contrast',
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
