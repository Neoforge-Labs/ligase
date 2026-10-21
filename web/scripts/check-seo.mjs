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
 *   3. Every page carries a canonical link, and it points at that page.
 *   4. The structured data is a SoftwareApplication that implies no
 *      adoption. The project has no outside users (CLAUDE.md), so there are
 *      no ratings, reviews, counts of interactions or downloads. Its
 *      version is Cargo.toml's, not a copy that goes stale.
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

// 3. canonicals ---------------------------------------------------------
const canonicalOf = (html) =>
  html.match(/<link[^>]+rel="canonical"[^>]*>/)?.[0].match(/href="([^"]+)"/)?.[1]
for (const page of exportedPages()) {
  const html = readFileSync(outPathFor(page), 'utf8')
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
for (const [path, md] of mds) {
  // An `import` is MDX only outside a code block: the guides' Python examples
  // legitimately start lines with `import ligase`.
  const prose = withoutCode(md)
  assert(!/^import\s.+\sfrom\s+['"]/m.test(prose), `${path}: an MDX import leaked into the markdown`)
  assert(!COMPONENT_TAG.test(prose), `${path}: a component tag leaked instead of its text`)
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
// The project has no outside users. Nothing published for a model may say
// otherwise. A phrase is only exempt where the page is disclaiming it: in
// quotation marks, in a sentence that says there is none ("no 'trusted by'
// list"). Every such mention is printed, so an exemption cannot go unseen.
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
const disclaimed = []
for (const [label, text] of [['llms.txt', llms], ['llms-full.txt', full]]) {
  for (const w of CLAIMS) {
    for (const m of text.matchAll(new RegExp(w.replace(/[-\s]/g, '[-\\s]'), 'gi'))) {
      const before = text.slice(Math.max(0, m.index - 80), m.index)
      const quoted = /["“‘']$/.test(before) && /^["”’']/.test(text.slice(m.index + w.length))
      const negated = /\b(no|not|never|without|none)\b[^.\n]*$/i.test(before)
      if (quoted && negated) disclaimed.push(`${label}: "${w}" ("...${before.slice(-40).replace(/\n/g, ' ')}${m[0]}...")`)
      else assert(false, `${label} claims "${w}": the project has no outside users (CLAUDE.md)`)
    }
  }
}

if (failures.length > 0) {
  console.error(`FAIL: ${failures.length} problem(s)`)
  for (const f of failures) console.error(`  - ${f}`)
  process.exit(1)
}
for (const d of disclaimed) console.log(`note: disclaimed, not claimed: ${d}`)
console.log(
  `OK: ${urls.length} sitemap URLs all exported, every exported page listed and canonical, ` +
    `AI crawlers named, structured data claims no adoption, llms.txt lists every page, ` +
    `${mds.length} markdown pages are free of JSX, prose claims no adoption.`,
)
