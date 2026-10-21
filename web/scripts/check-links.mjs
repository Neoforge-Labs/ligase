#!/usr/bin/env node
/**
 * Fail the build on a broken link, a dangling anchor, an unreachable page, or
 * a Markdown link that never resolved.
 *
 * `mkdocs build --strict` did the first and third of those, and that caught
 * real problems, so the replacement has to as well. Next.js checks none of
 * them: a static export is happy to emit an <a> pointing at nothing.
 *
 * This runs over the exported HTML in out/ rather than over the MDX sources,
 * which is the stricter place to look. It sees what a reader will click,
 * after every rewrite and component has had its say, and it can therefore
 * check anchors against the ids that were actually rendered.
 *
 * It also catches the class mkdocs --strict missed. A reference-style link
 * whose definition is absent -- `[like this][ref]` with no `[ref]: ...`
 * anywhere -- is not a broken link to Markdown. It is not a link at all: the
 * text passes through as literal square brackets and renders as prose. That
 * is how a broken link survived in the design decisions. Here, square brackets
 * surviving into the rendered text are treated as the mistake they almost
 * always are.
 *
 * External links are not fetched. A build gate that depends on the network
 * fails for reasons that have nothing to do with the commit.
 */

import { readdir, readFile } from 'node:fs/promises'
import { join, relative, resolve } from 'node:path'

const OUT = resolve(process.argv[2] ?? 'out')

/** Markdown that did not become a link. */
const UNRESOLVED = [
  { re: /\[[^\]\n]{1,120}\]\[[^\]\n]{0,120}\]/g, why: 'reference-style link with no definition' },
  { re: /\[[^\]\n]{1,120}\]\((?:\s*)\)/g, why: 'link with an empty target' },
  { re: /\]\(\s*(?:TODO|FIXME|\.\.\.)\s*\)/gi, why: 'placeholder link target' },
]

async function htmlFiles(dir) {
  const found = []
  for (const entry of await readdir(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) found.push(...(await htmlFiles(path)))
    else if (entry.name.endsWith('.html')) found.push(path)
  }
  return found
}

/** Everything inside <script>/<style>, and HTML comments, is not content. */
function stripNonContent(html) {
  return html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script>/gi, ' ')
    .replace(/<style\b[^>]*>[\s\S]*?<\/style>/gi, ' ')
    .replace(/<!--[\s\S]*?-->/g, ' ')
}

function textOf(html) {
  return stripNonContent(html)
    .replace(/<[^>]+>/g, ' ')
    .replace(/&amp;/g, '&')
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&#123;/g, '{')
    .replace(/&#125;/g, '}')
}

/** `out/guide/installation/index.html` -> `/guide/installation/` */
function routeOf(file) {
  const rel = relative(OUT, file).replaceAll('\\', '/')
  if (rel === 'index.html') return '/'
  if (rel.endsWith('/index.html')) return `/${rel.slice(0, -'index.html'.length)}`
  return `/${rel.slice(0, -'.html'.length)}`
}

function attrs(html, re) {
  return [...stripNonContent(html).matchAll(re)].map((m) => m[1])
}

const files = await htmlFiles(OUT)
if (files.length === 0) {
  console.error(`error: no HTML under ${OUT}. Build the site first.`)
  process.exit(1)
}

const pages = new Map()
for (const file of files) {
  const html = await readFile(file, 'utf8')
  // A redirect stub left by write-redirects.mjs. It is not a page: it has no
  // heading and nothing links to it on purpose, so the page rules below would
  // all misfire. What it does need is a target that exists.
  const redirect = /<html[^>]*\bdata-redirect-to="([^"]+)"/.exec(html)?.[1]
  pages.set(routeOf(file), {
    redirect,
    file,
    html,
    ids: new Set([
      ...attrs(html, /\bid="([^"]+)"/g),
      ...attrs(html, /\bname="([^"]+)"/g),
    ]),
    links: attrs(html, /<a\b[^>]*\bhref="([^"]*)"/g),
  })
}

/*
 * Nextra renders a "skip to content" link on every page, including its own
 * 404, where the target it points at is not rendered. That is the theme's
 * business, not this site's content.
 */
const THEME_ANCHORS = new Set(['nextra-skip-nav'])
const NOT_LINKED = new Set(['/', '/404', '/404/'])

const problems = []
const linkedTo = new Set(['/'])

for (const [route, page] of pages) {
  if (page.redirect) {
    // Checked as a redirect, not crawled as a page.
    if (!pages.has(page.redirect)) {
      problems.push(`${route}  redirects to ${page.redirect} -> no page there`)
    } else {
      linkedTo.add(page.redirect)
    }
    continue
  }

  // --- links -------------------------------------------------------------
  for (const href of page.links) {
    if (!href || href.startsWith('#')) {
      if (href.startsWith('#') && href.length > 1) {
        const id = decodeURIComponent(href.slice(1))
        if (!page.ids.has(id) && !THEME_ANCHORS.has(id)) {
          problems.push(`${route}  anchor #${id} does not exist on this page`)
        }
      }
      continue
    }
    if (/^(https?:|mailto:|tel:|data:)/i.test(href)) continue

    const [path, fragment] = href.split('#')
    // Resolve relative to the current route, which already ends in a slash.
    const target = path.startsWith('/')
      ? path
      : new URL(path, `http://x${route}`).pathname

    const normalised = target.endsWith('/') || target.includes('.') ? target : `${target}/`

    if (normalised.includes('.') && !normalised.endsWith('/')) {
      continue // a static asset, not a page
    }

    const destination = pages.get(normalised)
    if (!destination) {
      problems.push(`${route}  links to ${href} -> no page at ${normalised}`)
      continue
    }
    linkedTo.add(normalised)

    if (fragment) {
      const id = decodeURIComponent(fragment)
      if (!destination.ids.has(id) && !THEME_ANCHORS.has(id)) {
        problems.push(`${route}  links to ${href} -> ${normalised} has no anchor #${id}`)
      }
    }
  }

  // --- Markdown that never became a link ---------------------------------
  const text = textOf(page.html)
  for (const { re, why } of UNRESOLVED) {
    for (const match of text.matchAll(re)) {
      problems.push(`${route}  ${why}: ${match[0].trim().slice(0, 90)}`)
    }
  }
}

// --- pages nothing links to ------------------------------------------------
// The mkdocs equivalent was a page missing from the nav. Nextra already fails
// the build when _meta names a page that does not exist; this is the other
// direction -- a page that exists and that nothing reaches.
for (const [route, page] of pages) {
  if (NOT_LINKED.has(route) || page.redirect) continue
  if (!linkedTo.has(route)) {
    problems.push(`${route}  exists but nothing links to it, and it is in no nav`)
  }
}

if (problems.length) {
  console.error(`\n${problems.length} link problem(s):\n`)
  for (const problem of problems.sort()) console.error(`  ${problem}`)
  console.error('')
  process.exit(1)
}

const stubs = [...pages.values()].filter((p) => p.redirect).length
const anchors = [...pages.values()].reduce((n, p) => n + p.ids.size, 0)
const links = [...pages.values()].reduce((n, p) => n + p.links.length, 0)
console.log(
  `links ok: ${pages.size - stubs} pages, ${links} links, ${anchors} anchors, ` +
    `${stubs} redirects checked`,
)
