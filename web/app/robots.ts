import type { MetadataRoute } from 'next'

/*
 * The AI crawlers are named, each with its own stanza and an explicit Allow.
 * The `*` rule would already admit them, but a crawler that is not named is
 * left to whatever default it assumes, and these docs are written to be read,
 * quoted and cited by language models. Saying so is the point.
 *
 * Declared static for `output: 'export'`, which has no server to answer it.
 */
export const dynamic = 'force-static'

const AI_CRAWLERS = ['GPTBot', 'ClaudeBot', 'PerplexityBot', 'Google-Extended', 'CCBot']

export default function robots(): MetadataRoute.Robots {
  return {
    rules: [
      { userAgent: '*', allow: '/' },
      ...AI_CRAWLERS.map((userAgent) => ({ userAgent, allow: '/' })),
    ],
    sitemap: 'https://ligase.neoforgelabs.tech/sitemap.xml',
  }
}
