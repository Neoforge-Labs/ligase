/**
 * Fail when the site stops being discoverable, or starts claiming too much.
 *
 * Runs against `out/` after a build. It checks six things:
 *
 *   1. The sitemap and the exported pages agree, in both directions. Every
 *      URL it lists must exist as a page (a listed 404 is worse than an
 *      unlisted page), and every exported page must be listed (so a page
 *      that falls out of the page map is noticed, not silently dropped).
 *   2. robots.txt names each AI crawler with its own stanza and an explicit
 *      Allow, and points at the sitemap. An unnamed crawler is at the mercy
 *      of whatever default it assumes.
 *   3. Every page carries a canonical link and an og:url, and both point at
 *      that page rather than at the site. No two pages share a <title>.
 *   4. The structured data is a SoftwareApplication that implies no
 *      adoption, names the registries that hold the package, and every page
 *      but the front page and the 404 carries a BreadcrumbList whose every
 *      crumb is a page the sitemap lists. The project has no outside users
 *      (CLAUDE.md), so there are no ratings, reviews, counts of interactions
 *      or downloads. Its version is Cargo.toml's, not a copy that goes stale.
 *      Every section that holds pages is itself a page that links them, and
 *      the sitemap's dates are commit dates rather than the build's.
 *   5. The prose is published for models: llms.txt follows the convention and
 *      lists every page, llms-full.txt carries the real text, and every page
 *      has a markdown variant with no JSX, no MDX import and no component tag.
 *   6. None of that prose claims adoption the project does not have.
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
import { toPlainMarkdown, workspaceVersion } from './generate-llms.mjs'

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
 * in a sitemap. It is the only page allowed to be noindex: a content page that
 * turned noindex by accident must still fail. It is not exempt from anything
 * else: it declares a canonical (its own, /404/) like every other page.
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

// 3. canonicals, og:url, and one title each ----------------------------
const canonicalOf = (html) =>
  html.match(/<link[^>]+rel="canonical"[^>]*>/)?.[0].match(/href="([^"]+)"/)?.[1]
const ogUrlOf = (html) =>
  html.match(/<meta[^>]+property="og:url"[^>]*>/)?.[0].match(/content="([^"]+)"/)?.[1]
const titleOf = (html) => html.match(/<title>([^<]*)<\/title>/)?.[1]

/*
 * og:url is checked as strictly as the canonical because it is the address a
 * page claims everywhere rel=canonical is not read: link previews, and the
 * crawlers that take og:url as the page's identity. Declaring it once in the
 * root layout as a literal origin typechecks, deploys, passes every other
 * check here, and quietly tells all 110 pages to call themselves the
 * homepage. Only comparing it against the page it sits on catches that.
 */
const titles = new Map()
for (const page of exportedPages()) {
  const html = readFileSync(outPathFor(page), 'utf8')
  const canonical = canonicalOf(html)
  assert(canonical !== undefined, `${page} has no canonical link`)
  // A redirect stub canonicalises to its target by design, carries no
  // OpenGraph tags, and shares its title with every other stub.
  if (isRedirectStub(html)) continue
  if (canonical !== undefined) {
    assert(canonical === page, `${page} declares canonical ${canonical}, expected itself`)
  }

  const ogUrl = ogUrlOf(html)
  assert(ogUrl !== undefined, `${page} has no og:url`)
  if (ogUrl !== undefined) {
    assert(ogUrl === page, `${page} declares og:url ${ogUrl}, expected itself`)
  }

  const title = titleOf(html)
  assert(title !== undefined && title.trim() !== '', `${page} has no <title>`)
  if (title) titles.set(title, [...(titles.get(title) ?? []), page])
}

/*
 * A title shared by two pages leaves a search engine to pick which of them
 * to show for it, and it picks one. The pages that collided were a concept
 * and the API group of the same name, which are the two a reader is most
 * likely to want told apart.
 */
for (const [title, pages] of titles) {
  assert(
    pages.length === 1,
    `${pages.length} pages share the title "${title}" (${pages.join(', ')})`,
  )
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
/*
 * `sameAs` is the entity's identity, not just a repository link. The name
 * collides with DNA ligase, so the two registries that hold the package are
 * listed beside the repository: they are what lets a search engine or an
 * agent resolve "Ligase" to this software rather than to an enzyme. Each must
 * be present and each must be a real page — a withdrawn registry entry has to
 * come out of the layout, not be left pointing at a 404.
 */
const IDENTITY = [
  'https://github.com/Neoforge-Labs/ligase',
  'https://pypi.org/project/ligase-bio/',
  'https://crates.io/crates/ligase',
]
const sameAs = Array.isArray(ld.sameAs) ? ld.sameAs : [ld.sameAs]
for (const url of IDENTITY) {
  assert(sameAs.includes(url), `JSON-LD sameAs does not list ${url}`)
}
assert(
  String(ld.alternateName ?? '').includes('ligase-bio') ||
    (Array.isArray(ld.alternateName) && ld.alternateName.includes('ligase-bio')),
  'JSON-LD has no alternateName naming the PyPI distribution (ligase-bio)',
)
assert(ld.author?.name === 'Kelyn Njeri', 'JSON-LD author is not Kelyn Njeri')
assert(
  ld.softwareVersion === workspaceVersion(),
  `JSON-LD softwareVersion is ${ld.softwareVersion}, Cargo.toml says ${workspaceVersion()}`,
)
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

/*
 * Breadcrumbs. The theme draws a visible trail above every page, and this is
 * the markup for it, so it has to be a trail that can actually be walked:
 * positions 1..n in order, the first crumb the site root, the last the page
 * itself, and every crumb a page the sitemap lists. That last one is the
 * assertion that matters. Seven of the site's sections are folders with no
 * index.mdx, and a crumb pointing at one would hand a crawler a link to a
 * 404 dressed up as navigation -- the one way this markup can do harm.
 * components/Breadcrumbs.tsx leaves those sections out for that reason, and
 * this fails if it ever stops.
 */
for (const page of exportedPages()) {
  const html = readFileSync(outPathFor(page), 'utf8')
  if (isRedirectStub(html)) continue

  const found = jsonLdBlocks(html)
  const apps = found.filter((b) => b?.['@type'] === 'SoftwareApplication')
  assert(apps.length === 1, `${page} carries ${apps.length} SoftwareApplication blocks, expected 1`)
  const trails = found.filter((b) => b?.['@type'] === 'BreadcrumbList')

  // The front page is the root of every trail, and the 404 is noindex.
  if (page === `${ORIGIN}/` || NOINDEX_ALLOWED.has(page)) {
    assert(trails.length === 0, `${page} carries a breadcrumb, and it is not a page in a trail`)
    continue
  }

  assert(trails.length === 1, `${page} carries ${trails.length} BreadcrumbList blocks, expected 1`)
  const crumbs = trails[0]?.itemListElement ?? []
  assert(crumbs.length >= 2, `${page} has a breadcrumb of ${crumbs.length} crumb(s)`)
  crumbs.forEach((crumb, i) => {
    assert(crumb?.position === i + 1, `${page}: crumb ${i + 1} declares position ${crumb?.position}`)
    assert(
      typeof crumb?.name === 'string' && crumb.name.trim() !== '',
      `${page}: crumb ${i + 1} has no name`,
    )
    assert(
      typeof crumb?.item === 'string' && listed.has(crumb.item),
      `${page}: crumb ${i + 1} points at ${crumb?.item}, which the sitemap does not list`,
    )
  })
  assert(
    crumbs[0]?.item === `${ORIGIN}/`,
    `${page}: the trail starts at ${crumbs[0]?.item}, expected the site root`,
  )
  assert(
    crumbs.at(-1)?.item === page,
    `${page}: the trail ends at ${crumbs.at(-1)?.item}, expected the page itself`,
  )
}

// 4b. every section has a hub, and the hub links its section -----------
/*
 * A section that is only a folder is a hole in the site. The sidebar expands
 * it, so a reader never notices, but a crawler following links finds no page
 * at /concepts/ and the section has no address of its own to rank: the pages
 * inside it compete alone for a category query that none of them answers.
 * Seven sections were in that state, which is also why their breadcrumbs were
 * two crumbs deep — a trail cannot point at an address that is not a page.
 *
 * So: every directory that holds pages is itself a page, and that page links
 * to each page directly inside it.
 *
 * The check reads the hub's markdown variant, not its HTML. The HTML carries
 * the sidebar, which links every page on the site, so an HTML check would pass
 * on a hub whose body is empty. The .md is the page body and nothing else.
 *
 * The front page is exempt: it is a designed landing page, not a hub, and its
 * children are the sections themselves.
 */
const childrenOf = new Map() // section route -> [child page routes]
for (const u of urls) {
  const path = new URL(u).pathname
  if (path === '/') continue
  const parent = path.replace(/[^/]+\/$/, '')
  if (!childrenOf.has(parent)) childrenOf.set(parent, [])
  childrenOf.get(parent).push(path)
}
for (const [section, children] of childrenOf) {
  if (section === '/') continue // the front page is not a hub
  const hub = `${ORIGIN}${section}`
  assert(
    listed.has(hub),
    `${children.length} pages live under ${section} but it is not a page (add ${section.slice(1)}index.mdx and an "index" key to its _meta.ts)`,
  )
  const md = join(OUT, section, 'index.md')
  if (!listed.has(hub) || !existsSync(md)) continue
  const body = readFileSync(md, 'utf8')
  for (const child of children) {
    assert(
      body.includes(`${ORIGIN}${child}`),
      `${section} is a hub but its body does not link ${child}`,
    )
  }
}

// 4c. sitemap dates are real ---------------------------------------------
/*
 * `lastmod` is worth having only if it is true. The failure mode is emitting
 * the build time: every page then claims to have changed on every deploy, a
 * crawler that checked twice learns the value means nothing, and it stops
 * reading it — which is worse than never having sent one. So a date must parse,
 * must not be in the future, and the set of dates across the site must not be
 * a single value. lib/lastmod.ts reads them from git, and omits the date for a
 * page whose source is generated rather than committed.
 */
const stamps = [...sitemap.matchAll(/<lastmod>([^<]+)<\/lastmod>/g)].map((m) => m[1])
/*
 * Most pages must carry one. The way this breaks is not a wrong date but no
 * date at all: lib/lastmod.ts matches git's paths against the page's source
 * path, and when the two stop lining up every lookup misses and the map comes
 * back empty — which is valid XML and says nothing, so nothing else notices.
 * Half is the floor because the generated sections (the API pages, the CLI
 * reference, the notebooks) are gitignored and have no commit of their own.
 */
assert(
  stamps.length * 2 >= urls.length,
  `only ${stamps.length} of ${urls.length} sitemap URLs carry a lastmod — git paths and page paths have probably stopped matching in lib/lastmod.ts`,
)
{
  const now = Date.now()
  for (const stamp of stamps) {
    const when = new Date(stamp)
    assert(!Number.isNaN(when.getTime()), `sitemap has an unparseable lastmod: ${stamp}`)
    assert(when.getTime() <= now + 86_400_000, `sitemap dates a page in the future: ${stamp}`)
  }
  assert(
    new Set(stamps).size > 1,
    `all ${stamps.length} sitemap lastmod values are identical (${stamps[0]}) — that is a build timestamp, not a change date`,
  )
}

// 5. machine-readable prose ---------------------------------------------
const llms = read('llms.txt')
assert(/^# Ligase\n/.test(llms), 'llms.txt must open with "# Ligase"')
assert(/^> .+/m.test(llms), 'llms.txt needs a one-paragraph summary blockquote')
for (const section of ['Start here', 'Silent failures', 'Guides', 'Concepts', 'Reference', 'Notebooks', 'Project']) {
  assert(new RegExp(`^## ${section}$`, 'm').test(llms), `llms.txt has no "## ${section}" section (sections mirror the nav)`)
}
const entries = [...llms.matchAll(/^- \[(.+?)\]\((https:\/\/[^)\s]+)\): (.+)$/gm)]
assert(entries.length >= 55, `llms.txt lists ${entries.length} pages, expected at least 55`)

// Every page the sitemap lists is in llms.txt, as a link to its markdown, and
// every link lands on a file that exists. The page count itself is not
// asserted: the sitemap is the source of truth.
const linked = new Set(entries.map((e) => e[2]))
for (const u of urls) {
  const md = `${u}index.md`
  assert(linked.has(md), `${u} is in the sitemap but not in llms.txt`)
  assert(existsSync(join(OUT, new URL(md).pathname)), `${md} is not written (llms.txt links to it)`)
}
for (const [, title, link] of entries) {
  assert(link.startsWith(`${ORIGIN}/`) && link.endsWith('index.md'), `llms.txt links "${title}" to ${link}`)
  assert(existsSync(join(OUT, new URL(link).pathname)), `llms.txt links ${link}, which does not exist`)
}

const full = read('llms-full.txt')
assert(full.length > 150000, `llms-full.txt is ${full.length} characters; the site's prose should exceed 150,000`)
for (const u of urls) {
  assert(full.includes(`Source: ${u}\n`), `llms-full.txt does not contain ${u}`)
}

/** A markdown file with its code, fenced and inline, taken out. */
const withoutCode = (md) =>
  md
    .replace(/^(\s*)(`{3,}|~{3,})[^\n]*\n[\s\S]*?^\s*\2[`~]*\s*$/gm, '')
    .replace(/(`+)[\s\S]*?\1/g, '')

/** Every `index.md` under out/, as [path, text]. */
function generatedMarkdown() {
  const found = []
  const walk = (dir) => {
    for (const entry of readdirSync(dir)) {
      if (entry === '_next' || entry === '_pagefind') continue
      const full = join(dir, entry)
      if (statSync(full).isDirectory()) walk(full)
      else if (entry === 'index.md') found.push([relative(OUT, full), readFileSync(full, 'utf8')])
    }
  }
  walk(OUT)
  return found
}
const mds = generatedMarkdown()
assert(mds.length >= 55, `only ${mds.length} markdown pages were written`)

// A markdown variant sits beside every page that is in the sitemap.
for (const u of urls) {
  assert(existsSync(join(OUT, new URL(u).pathname, 'index.md')), `${u} has no index.md beside it`)
}

const COMPONENT_TAG = /<\/?(Diagnostic|RunReport|Hero|Band|Hazard|HazardGrid|Note|Stat|Stats|Lede|Action|BrandCard)\b/
const JSX_TAG = /<\/?[A-Z][A-Za-z0-9.]*(?=[\s/>])/
for (const [path, md] of mds) {
  // An `import` is MDX only outside a code block: the guides' Python examples
  // legitimately start lines with `import ligase`.
  const prose = withoutCode(md)
  assert(!/^import\s.+\sfrom\s+['"]/m.test(prose), `${path}: an MDX import leaked into the markdown`)
  assert(!COMPONENT_TAG.test(prose), `${path}: a component tag leaked instead of its text`)
  // Backstop for a component added later: generate-llms.mjs only converts the
  // ones it knows, so any other capitalised tag outside code is raw JSX.
  const unknownTag = JSX_TAG.exec(prose)
  assert(!unknownTag, `${path}: raw JSX outside a code block (${unknownTag?.[0]}); teach generate-llms.mjs the component`)
  assert(!/\{\s*(diagnostics|quickstart)\./.test(md), `${path}: an unresolved {diagnostics.*} or {quickstart.*} expression`)
  assert(!/<\/?>/.test(prose), `${path}: a JSX fragment leaked`)
  assert(!/^---\n(title|description):/m.test(md), `${path}: frontmatter leaked`)
  assert(!/\]\(\/(?!\/)/.test(prose), `${path}: a site-relative link (it breaks out of context)`)
  assert(/^# .+\n\nSource: https:\/\//.test(md), `${path}: does not open with its title and address`)
}

// The conversion keeps the text a component stands for, instead of just
// dropping the tag. The first case is the one that matters most: the
// diagnostic's text is not in the MDX at all, only a reference to it.
const real = mds.find(([p]) => p === 'silent-failures/contig-naming/index.md')?.[1] ?? ''
assert(/ContigNamingError in step `call`:/.test(real), 'contig-naming.md lost the diagnostic text')
assert(/Add a renaming step/.test(real), 'contig-naming.md lost the diagnostic advice')
const converted = toPlainMarkdown(`---
title: t
---

import { x } from '../x'

<Lede>Opening **prose**.</Lede>

<Note kind="tip" title="A title">
Body text.
</Note>

<HazardGrid>
  <Hazard href="/a/" symptom="It breaks" left="x" right="y">
    Because of this.
  </Hazard>
</HazardGrid>

<Stats>
  <Stat value="3 / 4" label="Things" note="somewhere" />
</Stats>

<Diagnostic note="nothing ran">{diagnostics.contigOrder}</Diagnostic>

\`Vec<Note>\` stays, and so does:

\`\`\`rust
fn f() -> Vec<Note> {}
import ligase as lg
\`\`\`
`)
for (const wanted of [
  'Opening **prose**.',
  '**Tip: A title**',
  'Body text.',
  '**It breaks**',
  'Because of this.',
  `${ORIGIN}/a/`,
  '**3 / 4** Things',
  'ContigOrderError in step `call`:',
  '`Vec<Note>` stays',
  'fn f() -> Vec<Note> {}',
  'import ligase as lg',
]) {
  assert(converted.includes(wanted), `toPlainMarkdown lost "${wanted}"`)
}
assert(!/import \{ x \}/.test(converted), 'toPlainMarkdown kept an MDX import')
assert(!COMPONENT_TAG.test(withoutCode(converted)), 'toPlainMarkdown left a component tag')

// 6. the honesty rule, automated (CLAUDE.md) ----------------------------
// The project has no outside users. This is a check for the specific phrases
// we know to avoid, not a complete guard: a claim worded some other way
// ("used by 40 labs") passes it. It fails closed on those phrases.
//
// One mention is allowed, and it is pinned: the sentence in the in-use page
// that says there is no "trusted by" list. The allowance is that exact
// sentence at that exact place, once. Any other occurrence of the phrase, or
// the same sentence a second time, or the phrase beside different words, is a
// failure. Widening this means editing ALLOWED below on purpose.
const CLAIMS = [
  'trusted by',
  'used by thousands',
  'industry-leading',
  'customers love',
  'widely used',
  'widely adopted',
  'production-ready',
  'production ready',
  'battle-tested',
  'adopted by',
  'used in production',
  'thousands of users',
  'loved by',
  'world-class',
]
const ALLOWED = [
  {
    file: 'llms-full.txt',
    phrase: 'trusted by',
    sentence: 'No logos, no quotes, no adoption figures, no "trusted by" list.',
    times: 1,
  },
]
const phrasePattern = (w) => new RegExp(w.replace(/[-\s]/g, '[-\\s]'), 'gi')
for (const [label, text] of [['llms.txt', llms], ['llms-full.txt', full]]) {
  for (const w of CLAIMS) {
    const allowed = ALLOWED.filter((a) => a.file === label && a.phrase === w)
    let exempted = 0
    for (const m of text.matchAll(phrasePattern(w))) {
      const hit = allowed.find((a) => {
        const at = m.index - a.sentence.indexOf(w)
        return at >= 0 && text.slice(at, at + a.sentence.length) === a.sentence
      })
      if (hit) {
        exempted += 1
        console.log(`note: exempted by the pinned allowlist, ${label}: ${hit.sentence}`)
      } else {
        const context = text.slice(Math.max(0, m.index - 40), m.index + w.length + 40).replace(/\s+/g, ' ')
        assert(false, `${label} claims "${w}" (...${context}...): the project has no outside users (CLAUDE.md)`)
      }
    }
    for (const a of allowed) {
      // At least once as well as at most: if the content page is reworded the
      // entry would otherwise sit here as dead config, still looking like a
      // standing exemption.
      assert(exempted >= 1, `${label}: the allowlisted sentence for "${w}" was never found, so the ALLOWED entry is dead config: ${a.sentence}`)
      assert(exempted <= a.times, `${label}: the allowlisted sentence for "${w}" appears ${exempted} times, expected at most ${a.times}`)
    }
  }
}

if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} problem(s)`)
  for (const f of failures) console.error(`  - ${f}`)
  process.exit(1)
}
console.log(
  `OK: ${urls.length} sitemap URLs all exported, every exported page listed, canonical, ` +
    `og:url'd to itself, uniquely titled and in a walkable breadcrumb, ` +
    `AI crawlers named, structured data claims no adoption, llms.txt lists every page, ` +
    `${mds.length} markdown pages are free of JSX, prose claims no adoption.`,
)
