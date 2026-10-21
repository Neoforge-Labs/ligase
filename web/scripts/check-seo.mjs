/**
 * Fail when the site stops being discoverable, or starts claiming too much.
 *
 * Runs against `out/` after a build. It checks four things:
 *
 *   1. The sitemap and the exported pages agree, in both directions. Every
 *      URL it lists must exist as a page (a listed 404 is worse than an
 *      unlisted page), and every exported page must be listed (so a page
 *      that falls out of the page map is noticed, not silently dropped).
 *   2. robots.txt names each AI crawler with its own stanza and an explicit
 *      Allow, and points at the sitemap. An unnamed crawler is at the mercy
 *      of whatever default it assumes.
 *   3. Every page carries a canonical link, and it points at that page.
 *   4. The structured data is a SoftwareApplication that implies no
 *      adoption. The project has no outside users (CLAUDE.md), so there are
 *      no ratings, reviews, counts of interactions or downloads.
 *
 * The count of pages is deliberately not asserted. The page map is the source
 * of truth, and a hardcoded number rots the day someone adds a page.
 *
 * Usage:
 *   node scripts/check-seo.mjs [out-dir]
 */
import { existsSync, readdirSync, readFileSync, statSync } from 'node:fs'
import { dirname, join, relative, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const OUT = resolve(process.argv[2] ?? join(root, 'out'))
const ORIGIN = 'https://ligase.neoforgelabs.tech'

const failures = []
function assert(cond, message) {
  if (!cond) failures.push(message)
}

const read = (p) => readFileSync(join(OUT, p), 'utf8')

for (const required of ['sitemap.xml', 'robots.txt', 'index.html']) {
  if (!existsSync(join(OUT, required))) {
    console.error(`FAIL: out/${required} does not exist (run "npm run build" first)`)
    process.exit(1)
  }
}

/** The file an exported URL is served from, under `trailingSlash: true`. */
function outPathFor(url) {
  const { pathname } = new URL(url)
  return join(OUT, pathname, pathname.endsWith('/') ? 'index.html' : '')
}

/** Every `index.html` under out/, as the URL it is served at. */
function exportedPages() {
  const pages = []
  const walk = (dir) => {
    for (const entry of readdirSync(dir)) {
      // Build machinery, not pages: the search index and Next's own assets.
      if (entry === '_next' || entry === '_pagefind') continue
      const full = join(dir, entry)
      if (statSync(full).isDirectory()) walk(full)
      else if (entry === 'index.html') {
        const rel = relative(OUT, dir)
        pages.push(rel === '' ? `${ORIGIN}/` : `${ORIGIN}/${rel.split('\\').join('/')}/`)
      }
    }
  }
  walk(OUT)
  return pages
}

/** A redirect stub (written by write-redirects.mjs) is not a page of its own. */
const isRedirectStub = (html) => /<meta[^>]+http-equiv="refresh"/i.test(html)

/**
 * The 404 is exported as a page and Next marks it noindex, so it has no place
 * in a sitemap and its canonical (the layout's, resolved against a route that
 * does not exist) is moot. It is the only page allowed to be exempt: a content
 * page that turned noindex by accident must still fail.
 */
const isNoindex = (html) => /<meta[^>]+name="robots"[^>]+content="[^"]*noindex/i.test(html)
const NOINDEX_ALLOWED = new Set([`${ORIGIN}/404/`])

// 1. sitemap <-> exported pages -----------------------------------------
const sitemap = read('sitemap.xml')
const urls = [...sitemap.matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1])
assert(urls.length > 0, 'sitemap.xml lists no URLs')
assert(new Set(urls).size === urls.length, 'sitemap.xml lists a URL more than once')

for (const u of urls) {
  assert(u.startsWith(`${ORIGIN}/`), `sitemap lists ${u}, which is not on ${ORIGIN}`)
  assert(u.endsWith('/'), `sitemap lists ${u} without the trailing slash the export uses`)
  assert(existsSync(outPathFor(u)), `sitemap lists ${u} but no page was exported`)
}

const listed = new Set(urls)
for (const page of exportedPages()) {
  const html = readFileSync(outPathFor(page), 'utf8')
  if (isRedirectStub(html)) continue
  if (isNoindex(html)) {
    assert(NOINDEX_ALLOWED.has(page), `${page} is noindex but is not the 404 page`)
    continue
  }
  assert(listed.has(page), `${page} was exported but is missing from the sitemap`)
}
assert(!urls.some((u) => /\/404\/?$/.test(u)), 'sitemap lists the 404 page')

// 2. robots.txt ---------------------------------------------------------
const robots = read('robots.txt')
assert(/^User-agent:\s*\*\s*$/im.test(robots), 'robots.txt has no stanza for *')

/** The directives of the stanza that opens with `User-agent: <bot>`. */
function stanzaFor(bot) {
  const blocks = robots.split(/\n\s*\n/)
  return (
    blocks.find((b) =>
      new RegExp(`^User-agent:\\s*${bot}\\s*$`, 'im').test(b),
    ) ?? ''
  )
}
for (const bot of ['GPTBot', 'ClaudeBot', 'PerplexityBot', 'Google-Extended', 'CCBot']) {
  const stanza = stanzaFor(bot)
  assert(stanza !== '', `${bot} not named in robots.txt`)
  assert(/^Allow:\s*\/\s*$/im.test(stanza), `${bot} is named but not explicitly allowed`)
  assert(!/^Disallow:\s*\/\s*$/im.test(stanza), `${bot} is disallowed from the whole site`)
}
assert(
  new RegExp(`^Sitemap:\\s*${ORIGIN.replace(/\./g, '\\.')}/sitemap\\.xml\\s*$`, 'im').test(robots),
  'robots.txt does not point at the sitemap',
)

// 3. canonicals ---------------------------------------------------------
const canonicalOf = (html) =>
  html.match(/<link[^>]+rel="canonical"[^>]*>/)?.[0].match(/href="([^"]+)"/)?.[1]
for (const page of exportedPages()) {
  const html = readFileSync(outPathFor(page), 'utf8')
  if (isNoindex(html) && NOINDEX_ALLOWED.has(page)) continue
  const canonical = canonicalOf(html)
  assert(canonical !== undefined, `${page} has no canonical link`)
  // A redirect stub canonicalises to its target by design.
  if (canonical !== undefined && !isRedirectStub(html)) {
    assert(canonical === page, `${page} declares canonical ${canonical}, expected itself`)
  }
}

// 4. structured data ----------------------------------------------------
function jsonLdBlocks(html) {
  return [
    ...html.matchAll(/<script[^>]+type="application\/ld\+json"[^>]*>([\s\S]*?)<\/script>/g),
  ].map((m) => JSON.parse(m[1]))
}
const blocks = jsonLdBlocks(read('index.html'))
assert(blocks.length === 1, `index.html carries ${blocks.length} JSON-LD blocks, expected 1`)
const ld = blocks[0] ?? {}
assert(ld['@type'] === 'SoftwareApplication', 'JSON-LD is not a SoftwareApplication')
assert(ld.name === 'Ligase', 'JSON-LD name is not Ligase')
assert(ld.sameAs === 'https://github.com/Neoforge-Labs/ligase', 'JSON-LD repository (sameAs) is wrong')
assert(ld.author?.name === 'Kelyn Njeri', 'JSON-LD author is not Kelyn Njeri')
assert(ld.programmingLanguage !== undefined, 'JSON-LD has no programmingLanguage')
assert(ld.applicationCategory !== undefined, 'JSON-LD has no applicationCategory')
assert(ld.operatingSystem !== undefined, 'JSON-LD has no operatingSystem')
const licence = JSON.stringify(ld.license ?? '')
assert(/MIT/.test(licence) && /Apache-2\.0/.test(licence), 'JSON-LD licence is not MIT and Apache-2.0')

// The project has no outside users. None of these may appear, at any depth.
const ADOPTION = [
  'aggregateRating',
  'review',
  'rating',
  'ratingValue',
  'ratingCount',
  'reviewCount',
  'interactionCount',
  'interactionStatistic',
  'userInteractionCount',
  'downloadCount',
  'installCount',
  'downloadUrl',
  'award',
  'offers',
]
function keysOf(value, found = new Set()) {
  if (Array.isArray(value)) value.forEach((v) => keysOf(v, found))
  else if (value && typeof value === 'object') {
    for (const [k, v] of Object.entries(value)) {
      found.add(k)
      keysOf(v, found)
    }
  }
  return found
}
const present = keysOf(ld)
for (const key of ADOPTION) {
  assert(!present.has(key), `JSON-LD carries "${key}": the project has no outside users (CLAUDE.md)`)
}
// ...and the prose must not claim what the project is not.
const prose = String(ld.description ?? '')
for (const word of ['production-ready', 'production ready', 'trusted', 'popular', 'widely used']) {
  assert(!prose.toLowerCase().includes(word), `JSON-LD description claims "${word}"`)
}

if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} problem(s)`)
  for (const f of failures) console.error(`  - ${f}`)
  process.exit(1)
}
console.log(
  `OK: ${urls.length} sitemap URLs all exported, every exported page listed and canonical, ` +
    'AI crawlers named, structured data claims no adoption.',
)
