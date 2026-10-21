/**
 * Fail when the journal furniture in globals.css stops meeting the spec.
 *
 * The furniture is the typographic apparatus that makes the docs read as a
 * research journal: the orange eyebrow, the muted mono label, the numbered
 * sections and figures, the grain and the aurora. Its values are measured
 * from Neoforge Labs' own source, so they are asserted exactly rather than
 * approximately.
 *
 * Beyond the values, this guards the four ways the furniture can hurt a
 * reader:
 *   - the grain and aurora must switch off under prefers-reduced-motion,
 *     inside the existing block rather than a second one;
 *   - a print stylesheet must force ink on paper, because the site defaults
 *     to near-black and 52,000 words of prose get printed;
 *   - the section counter must restart on every page and must only ever be
 *     incremented by an h2 inside .nextra-content, so a generated page with
 *     no h2 (or one that starts at h3) never shows a stray or zero count;
 *   - body prose stays in the serif; the display face is for h1 and display
 *     titles only.
 *
 * Usage:
 *   node scripts/check-furniture.mjs
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')

const failures = []
function assert(cond, message) {
  if (!cond) failures.push(message)
}

let raw = ''
try {
  raw = readFileSync(join(root, 'app/globals.css'), 'utf8')
} catch (err) {
  console.error(`FAIL: ${err.message}`)
  process.exit(1)
}

// Comments are prose, and prose mentions the very strings we assert on.
const css = raw.replace(/\/\*[\s\S]*?\*\//g, '')

/** The body of the brace block that opens at `openIndex`, or null. */
function blockAt(text, openIndex) {
  let depth = 0
  for (let i = openIndex; i < text.length; i++) {
    if (text[i] === '{') depth++
    else if (text[i] === '}' && --depth === 0) return text.slice(openIndex + 1, i)
  }
  return null
}

/** Every at-rule block whose prelude matches `prelude` (a string). */
function atRules(prelude) {
  const out = []
  let from = 0
  for (;;) {
    const at = css.indexOf(prelude, from)
    if (at === -1) return out
    const open = css.indexOf('{', at)
    const body = blockAt(css, open)
    if (body !== null) out.push(body)
    from = open + 1
  }
}

/** Declarations of every rule (outside or inside `scope`) with exactly this selector list. */
function rules(scope, selector) {
  const wanted = selector.split(',').map((s) => s.trim().replace(/\s+/g, ' '))
  const found = []
  const re = /([^{}]+)\{([^{}]*)\}/g
  for (let m; (m = re.exec(scope)); ) {
    const sel = m[1].split(',').map((s) => s.trim().replace(/\s+/g, ' '))
    if (wanted.every((w) => sel.includes(w))) found.push(m[2])
  }
  return found
}
const has = (bodies, re) => bodies.some((b) => re.test(b))

// --- Measured values, exactly --------------------------------------------

const eyebrow = rules(css, '.lg-eyebrow')
assert(has(eyebrow, /letter-spacing:\s*0\.28em/), '.lg-eyebrow must set letter-spacing: 0.28em')
assert(has(eyebrow, /font-size:\s*10px/), '.lg-eyebrow must set font-size: 10px')
assert(has(eyebrow, /text-transform:\s*uppercase/), '.lg-eyebrow must be uppercase')
assert(has(eyebrow, /color:\s*var\(--lg-brand\)/), '.lg-eyebrow must be coloured var(--lg-brand)')
assert(
  has(eyebrow, /font-family:\s*var\(--lg-font-mono\)/),
  '.lg-eyebrow must be set in var(--lg-font-mono)',
)

const mono = rules(css, '.lg-mono')
assert(has(mono, /letter-spacing:\s*0\.22em/), '.lg-mono must set letter-spacing: 0.22em')
assert(has(mono, /font-size:\s*11px/), '.lg-mono must set font-size: 11px')
assert(has(mono, /text-transform:\s*uppercase/), '.lg-mono must be uppercase')
assert(has(mono, /color:\s*var\(--lg-ink-muted\)/), '.lg-mono must be coloured var(--lg-ink-muted)')
assert(has(mono, /font-family:\s*var\(--lg-font-mono\)/), '.lg-mono must be set in var(--lg-font-mono)')

const grain = rules(css, '.lg-grain::before')
assert(grain.length > 0, '.lg-grain::before is missing')
assert(has(grain, /z-index:\s*9999/), '.lg-grain::before must sit at z-index: 9999')
assert(has(grain, /background-repeat:\s*repeat/), '.lg-grain::before must repeat its tile')
assert(has(grain, /position:\s*fixed/), '.lg-grain::before must be fixed to the viewport')
assert(has(grain, /pointer-events:\s*none/), '.lg-grain::before must not intercept pointer events')
assert(has(grain, /opacity:\s*0\.035/), '.lg-grain::before must set opacity: 0.035')
assert(has(grain, /feTurbulence/), '.lg-grain::before must draw its noise with feTurbulence')
assert(has(grain, /fractalNoise/), '.lg-grain::before must use fractalNoise')
assert(has(grain, /baseFrequency=['"%27]*0\.85/), '.lg-grain::before must use baseFrequency 0.85')
assert(has(grain, /numOctaves=['"%27]*4/), '.lg-grain::before must use numOctaves 4')
assert(has(grain, /background-size:\s*256px\s+256px/), '.lg-grain::before must tile at 256px')

const aurora = rules(css, '.lg-aurora')
assert(aurora.length > 0, '.lg-aurora is missing')
assert(has(aurora, /filter:\s*blur\(60px\)/), '.lg-aurora must set filter: blur(60px)')
assert(has(aurora, /pointer-events:\s*none/), '.lg-aurora must not intercept pointer events')
assert(has(aurora, /position:\s*absolute/), '.lg-aurora must be position: absolute')
for (const a of ['0.08', '0.05', '0.04']) {
  assert(
    has(aurora, new RegExp(`radial-gradient\\([^;]*rgba\\(227, 100, 20, ${a.replace('.', '\\.')}\\)`)),
    `.lg-aurora must have an orange radial at ${a}`,
  )
}
assert(
  (aurora.join('').match(/radial-gradient\(/g) ?? []).length === 3,
  '.lg-aurora must have exactly three radials',
)

// --- Review focus 2: reduced motion, extended not duplicated -------------

const reduced = atRules('@media (prefers-reduced-motion: reduce)')
assert(
  reduced.length === 1,
  `expected exactly one prefers-reduced-motion block, found ${reduced.length}`,
)
const reducedCss = reduced.join('\n')
assert(
  has(rules(reducedCss, '.lg-grain::before'), /display:\s*none/),
  'reduced motion must set .lg-grain::before { display: none }',
)
assert(
  has(rules(reducedCss, '.lg-aurora'), /display:\s*none/),
  'reduced motion must set .lg-aurora { display: none }',
)
assert(
  /transition-duration:\s*0\.01ms\s*!important/.test(reducedCss) &&
    /animation-duration:\s*0\.01ms\s*!important/.test(reducedCss),
  'the existing reduced-motion duration rule must survive the extension',
)

// --- Review focus 3: print forces ink on paper ---------------------------

const prints = atRules('@media print')
assert(prints.length === 1, `expected exactly one @media print block, found ${prints.length}`)
const printCss = prints.join('\n')
assert(
  has(rules(printCss, 'html, body'), /background:\s*#fff\b/) &&
    has(rules(printCss, 'html, body'), /color:\s*#000\b/),
  'print must force html, body to background: #fff and color: #000',
)
for (const sel of ['.lg-grain', '.lg-aurora', '.nextra-sidebar-container', '.nextra-toc']) {
  assert(
    has(rules(printCss, sel), /display:\s*none/),
    `print must hide ${sel}`,
  )
}
assert(
  has(rules(printCss, '.nextra-content h2::before'), /color:\s*(#000|inherit)\b/),
  'print must drop the section counter colour to ink',
)

// --- Review focus 4: counters restart per page and never leak -----------

const resets = [...css.matchAll(/counter-reset:\s*([^;}]+)/g)].map((m) => ({
  names: m[1].trim().split(/\s+/),
}))
assert(
  has(rules(css, '.nextra-content'), /counter-reset:\s*lg-section\s+lg-figure\s*;?/),
  '.nextra-content must reset lg-section and lg-figure together',
)
assert(
  resets.length === 1,
  `lg-section and lg-figure must be reset in one place, found ${resets.length} counter-reset declarations`,
)

const incSection = [...css.matchAll(/([^{}]+)\{[^{}]*counter-increment:\s*lg-section\b[^{}]*\}/g)]
assert(incSection.length === 1, `lg-section must be incremented exactly once, found ${incSection.length}`)
assert(
  incSection.every((m) => m[1].trim() === '.nextra-content h2::before'),
  'lg-section must be incremented only by ".nextra-content h2::before"',
)
const sectionBefore = rules(css, '.nextra-content h2::before')
assert(
  has(sectionBefore, /content:\s*'§ '\s*counter\(lg-section,\s*decimal-leading-zero\)\s*;/),
  'the h2 section marker must read "§ NN", with no trailing dash, from lg-section',
)
assert(has(sectionBefore, /display:\s*block/), 'the section marker must sit above the heading (display: block)')
assert(has(sectionBefore, /color:\s*var\(--lg-brand\)/), 'the section marker must be brand-coloured')
assert(has(sectionBefore, /letter-spacing:\s*0\.28em/), 'the section marker must be set like .lg-eyebrow')

const figure = rules(css, '.lg-figure::before')
assert(
  has(figure, /counter-increment:\s*lg-figure/) &&
    has(figure, /content:\s*'FIG\. '\s*counter\(lg-figure,\s*decimal-leading-zero\)/),
  '.lg-figure::before must count figures as "FIG. NN"',
)
assert(has(figure, /letter-spacing:\s*0\.22em/), '.lg-figure::before must be set like .lg-mono')

// --- Divergence guard: Playfair is for h1 and display titles only --------

const prose = rules(css, '.nextra-content :is(p, li, dd, blockquote, td, th)')
assert(
  has(prose, /font-family:\s*var\(--lg-font-prose\)/),
  'body prose must stay in var(--lg-font-prose)',
)
assert(
  has(prose, /font-size:\s*var\(--lg-prose-size\)/),
  'body prose size must stay var(--lg-prose-size)',
)
assert(
  has(rules(css, '.nextra-content p'), /max-width:\s*var\(--lg-measure\)/),
  'body prose measure must stay var(--lg-measure)',
)

const displayUsers = [...css.matchAll(/([^{}]+)\{[^{}]*font-family:\s*var\(--lg-font-display\)[^{}]*\}/g)]
  .map((m) => m[1].trim().replace(/\s+/g, ' '))
  .sort()
assert(
  JSON.stringify(displayUsers) === JSON.stringify(['.lg-display', '.nextra-content h1']),
  `var(--lg-font-display) is for h1 and .lg-display only, found: ${displayUsers.join(' | ') || 'none'}`,
)
const h1 = rules(css, '.nextra-content h1')
assert(has(h1, /font-weight:\s*300/), 'h1 must be weight 300')
assert(has(h1, /letter-spacing:\s*-0\.02em/), 'h1 must set letter-spacing: -0.02em')
const display = rules(css, '.lg-display')
assert(has(display, /font-weight:\s*300/), '.lg-display must be weight 300')
assert(has(display, /letter-spacing:\s*-0\.02em/), '.lg-display must set letter-spacing: -0.02em')

// --- The header comment must describe the ground the file now uses -------

const header = raw.match(/^\s*\/\*[\s\S]*?\*\//)?.[0] ?? ''
assert(header.length > 0, 'globals.css must open with a header comment')
assert(!/warm paper/i.test(raw), 'a comment still claims warm paper')
assert(/near-black/i.test(header), 'the header comment must say the ground is near-black')
assert(/cream/i.test(header), 'the header comment must say cream is kept for reading and printing')

if (failures.length) {
  console.error(`check-furniture: ${failures.length} failure(s)`)
  for (const f of failures) console.error(`  FAIL: ${f}`)
  process.exit(1)
}
console.log('check-furniture: ok')
