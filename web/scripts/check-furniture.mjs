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
 * It also guards the components that carry the engine's voice (Diagnostic,
 * RunReport, Note, Stats): that they number themselves as figures in a slot
 * of their own, that no MDX call site lost the `note` text it passes, that
 * the default callout is off the brand colour, and that print resets every
 * token a panel is drawn with.
 *
 * Usage:
 *   node scripts/check-furniture.mjs
 */
import { readdirSync, readFileSync, statSync } from 'node:fs'
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
assert(has(h1, /font-weight:\s*400/), 'h1 must be weight 400, the display face\'s lightest')
assert(has(h1, /letter-spacing:\s*-0\.02em/), 'h1 must set letter-spacing: -0.02em')
const display = rules(css, '.lg-display')
assert(has(display, /font-weight:\s*400/), '.lg-display must be weight 400, the display face\'s lightest')
assert(has(display, /letter-spacing:\s*-0\.02em/), '.lg-display must set letter-spacing: -0.02em')

// --- The header comment must describe the ground the file now uses -------

const header = raw.match(/^\s*\/\*[\s\S]*?\*\//)?.[0] ?? ''
assert(header.length > 0, 'globals.css must open with a header comment')
assert(!/warm paper/i.test(raw), 'a comment still claims warm paper')
assert(/near-black/i.test(header), 'the header comment must say the ground is near-black')
assert(/cream/i.test(header), 'the header comment must say cream is kept for reading and printing')

// --- Components: the engine's panels, numbered as figures ----------------

const read = (rel) => readFileSync(join(root, rel), 'utf8')
const stripComments = (text) => text.replace(/\/\*[\s\S]*?\*\//g, '')

function walk(dir, out = []) {
  for (const name of readdirSync(dir)) {
    const full = join(dir, name)
    if (statSync(full).isDirectory()) walk(full, out)
    else if (full.endsWith('.mdx')) out.push(full)
  }
  return out
}
// content/api, content/notebooks and the CLI page are generated and may be
// absent; the hand-written pages are what carry the call sites.
const mdx = walk(join(root, 'content')).map((f) => readFileSync(f, 'utf8'))

const diagnosticCss = stripComments(read('components/Diagnostic.module.css'))
const runReportCss = stripComments(read('components/RunReport.module.css'))
const noteCss = stripComments(read('components/Note.module.css'))
const statsCss = stripComments(read('components/Stats.module.css'))
const diagnosticTsx = read('components/Diagnostic.tsx')
const runReportTsx = read('components/RunReport.tsx')

// The call sites' note text must survive untouched: the figure number gets a
// slot of its own and never replaces what the page says produced the output.
assert(
  mdx.filter((f) => /note="ligase check — nothing ran"/.test(f)).length === 6,
  'six pages must still pass note="ligase check — nothing ran" to <Diagnostic>',
)
assert(
  mdx.some((f) => /note="nothing ran"/.test(f)),
  'content/start/quickstart.mdx must still pass note="nothing ran"',
)

// FIG. gets its own slot, and RunReport gains a bar it did not have.
assert(/:global\(\.lg-figure\)/.test(diagnosticCss), 'Diagnostic.module.css must position .lg-figure')
assert(/:global\(\.lg-figure\)/.test(runReportCss), 'RunReport.module.css must position .lg-figure')
assert(/counter-increment:\s*lg-figure/.test(css), 'globals.css must increment the lg-figure counter')
for (const [name, tsx] of [
  ['Diagnostic', diagnosticTsx],
  ['RunReport', runReportTsx],
]) {
  assert(/figure\s*=\s*true/.test(tsx), `${name} must default figure to true`)
  assert(/figure\?:\s*boolean/.test(tsx), `${name} must declare figure?: boolean`)
  assert(/className="lg-figure"/.test(tsx), `${name} must render the global lg-figure class`)
}
assert(/\{note\s*\?/.test(diagnosticTsx), 'Diagnostic must still render its note')
assert(
  diagnosticTsx.indexOf('lg-figure') < diagnosticTsx.indexOf('styles.barLeft'),
  'Diagnostic must put the figure label before .barLeft, at the start of the bar',
)
assert(/note\?:\s*ReactNode/.test(runReportTsx), 'RunReport must accept note?: ReactNode')
assert(/styles\.bar\b/.test(runReportTsx), 'RunReport must render a bar')
assert(
  has(rules(runReportCss, '.bar'), /border-bottom:\s*1px solid var\(--lg-rule\)/),
  'the RunReport bar must be ruled off from its body like the Diagnostic bar',
)

// Panels are plates: a raised sheet inside a hairline frame.
for (const [name, text] of [
  ['Diagnostic', diagnosticCss],
  ['RunReport', runReportCss],
]) {
  const panel = rules(text, '.panel')
  assert(has(panel, /background:\s*var\(--lg-paper-raised\)/), `${name} .panel must sit on --lg-paper-raised`)
  assert(has(panel, /border:\s*1px solid var\(--lg-rule\)/), `${name} .panel must have a 1px --lg-rule frame`)
  assert(has(panel, /border-radius:\s*var\(--lg-radius-lg\)/), `${name} .panel must use --lg-radius-lg`)
}

// Review focus: the default callout is no longer on brand, in any property.
const noteKindRules = [
  ...noteCss.matchAll(/([^{}]*\[data-kind=['"]note['"]\][^{}]*)\{([^{}]*)\}/g),
]
assert(noteKindRules.length >= 2, "Note.module.css must style [data-kind='note'] and its label")
for (const [, sel, body] of noteKindRules) {
  assert(!/--lg-brand/.test(body), `Note rule "${sel.trim()}" must not use the brand colour`)
}
const noteRule = noteKindRules.find(([, sel]) => !/\.label/.test(sel))?.[2] ?? ''
assert(/--lg-rule-strong/.test(noteRule), "Note kind='note' must have a --lg-rule-strong rule")
assert(/background:\s*var\(--lg-paper-sunken\)/.test(noteRule), "Note kind='note' must sit on --lg-paper-sunken")
assert(/color:\s*var\(--lg-ink\)/.test(noteRule), "Note kind='note' must set --lg-ink text")
for (const kind of ['warning', 'tip', 'caution']) {
  assert(
    new RegExp(`\\[data-kind=['"]${kind}['"]\\][^{}]*\\{[^{}]*--lg-(refuse|agree|notice)\\b`).test(noteCss),
    `Note kind='${kind}' must keep its signal colour`,
  )
}

// The display face must only be asked for weights layout.tsx loads. Playfair
// Display starts at 400; a rule asking for 300 builds nothing and falls back
// silently, and no build can run here to say so.
const layoutSrc = read('app/layout.tsx')
const playfairCall = layoutSrc.match(/Playfair_Display\(\{([\s\S]*?)\}\)/)?.[1] ?? ''
const loadedWeights = [...(playfairCall.match(/weight:\s*\[([^\]]*)\]/)?.[1] ?? '').matchAll(/['"](\d+)['"]/g)].map(
  (m) => m[1],
)
assert(loadedWeights.length > 0, 'layout.tsx must list the Playfair_Display weights it loads')
assert(
  loadedWeights.every((w) => Number(w) >= 400),
  `Playfair Display has no weight below 400; layout.tsx lists ${loadedWeights.join(', ')}`,
)
const cssFiles = [
  ['app/globals.css', css],
  ...readdirSync(join(root, 'components'))
    .filter((f) => f.endsWith('.css'))
    .map((f) => [`components/${f}`, stripComments(read(`components/${f}`))]),
]
for (const [file, text] of cssFiles) {
  for (const m of text.matchAll(/([^{}]+)\{([^{}]*font-family:\s*var\(--lg-font-display\)[^{}]*)\}/g)) {
    const sel = m[1].trim().replace(/\s+/g, ' ')
    const weight = m[2].match(/font-weight:\s*(\d+)/)?.[1]
    assert(weight !== undefined, `${file}: "${sel}" sets the display face without an explicit font-weight`)
    assert(
      weight === undefined || loadedWeights.includes(weight),
      `${file}: "${sel}" asks the display face for weight ${weight}, but layout.tsx loads only ${loadedWeights.join(', ')}`,
    )
  }
}

// The figure label's divider only exists to separate it from what follows.
for (const [name, text] of [
  ['Diagnostic', diagnosticCss],
  ['RunReport', runReportCss],
]) {
  const figRule = text.match(/:global\(\.lg-figure\)\s*\{([^}]*)\}/)?.[1] ?? ''
  assert(!/border-right/.test(figRule), `${name}: the figure label must not carry an unconditional border-right`)
  assert(
    /:global\(\.lg-figure\):not\(:last-child\)/.test(text),
    `${name}: the figure label's divider must be suppressed when nothing follows it`,
  )
}

// Stats: measured figures in the display face, still tabular so they align.
const statValue = rules(statsCss, '.value')
assert(has(statValue, /font-family:\s*var\(--lg-font-display\)/), 'Stats .value must use the display face')
assert(has(statValue, /font-weight:\s*400/), 'Stats .value must be weight 400')
assert(has(statValue, /tabular-nums/), 'Stats .value must keep tabular-nums')

// Print: every token a panel is drawn with must be reset, not only the neutrals.
for (const family of ['refuse', 'agree', 'notice']) {
  for (const suffix of ['', '-soft', '-rule']) {
    assert(
      new RegExp(`--lg-${family}${suffix}:\\s*#`).test(printCss),
      `print must reset --lg-${family}${suffix}`,
    )
  }
}
assert(
  !/hue of its own except the grain/.test(header),
  'the header comment must not say the grain picks a hue',
)
assert(!/grain[^.]*brand orange/i.test(header), 'the grain is noise, not brand orange')

// --- The card lockup and the page that documents it ---------------------

const cardCss = stripComments(read('components/BrandCard.module.css'))
const cardTsx = read('components/BrandCard.tsx')
assert(/aspect-ratio:\s*3\s*\/\s*4/.test(cardCss), 'BrandCard must be a 3/4 portrait')
assert(
  has(rules(cardCss, '.hairline'), /width:\s*48px/) &&
    has(rules(cardCss, '.hairline'), /height:\s*1px/),
  'the BrandCard hairline must start at 1px by 48px',
)
assert(/hairline[^{}]*\{[^{}]*width:\s*96px/.test(cardCss), 'the BrandCard hairline must grow to 96px on hover')
assert(has(rules(cardCss, '.hairline'), /transition:\s*width\s+500ms/), 'the hairline must grow over 500ms')
assert(has(rules(cardCss, '.card'), /border-top:\s*2px solid/), 'BrandCard must carry a 2px top rule in its colour')
assert(has(rules(cardCss, '.card'), /padding:\s*1\.5rem/), 'BrandCard padding must start at 1.5rem')
assert(/padding:\s*2rem/.test(cardCss), 'BrandCard padding must grow to 2rem')
assert(has(rules(cardCss, '.tagline'), /font-size:\s*0\.875rem/), 'the BrandCard tagline must be 0.875rem')
assert(has(rules(cardCss, '.title'), /font-weight:\s*400/), 'the BrandCard title must be weight 400')
assert(!/url\(|linear-gradient|background-image/.test(cardCss), 'BrandCard has no image and no scrim')
assert(/className=\{`lg-mono /.test(cardTsx), 'the BrandCard code row must use the global .lg-mono')
assert(/BrandCard/.test(read('mdx-components.tsx')), 'BrandCard must be registered in mdx-components.tsx')

assert(/identity/.test(read('content/project/architecture/_meta.ts')), 'identity.mdx must be in the architecture _meta.ts, or nothing reaches it')
assert(mdx.some((f) => /<BrandCard\b/.test(f)), 'the identity page must render the lockup, not only describe it')

// The Band heading is not a numbered section of the journal.
assert(
  has(rules(stripComments(read('components/Hero.module.css')), '.band .bandTitle::before'), /content:\s*none/),
  "a Band's heading must not pick up the global section marker",
)

// Atmosphere that nothing mounts is dead CSS the identity page would still be
// describing. Both classes must be used by markup, and the aurora's parent must
// clip it.
const markup = [
  read('app/layout.tsx'),
  ...readdirSync(join(root, 'components'))
    .filter((f) => f.endsWith('.tsx'))
    .map((f) => read(`components/${f}`)),
].join('\n')
assert(/className="[^"]*\blg-grain\b/.test(read('app/layout.tsx')), 'layout.tsx must mount .lg-grain on body')
assert(/className="[^"]*\blg-aurora\b/.test(markup), '.lg-aurora must be mounted by a component')
const heroCss = stripComments(read('components/Hero.module.css'))
assert(
  has(rules(heroCss, '.hero'), /position:\s*relative/) && has(rules(heroCss, '.hero'), /overflow:\s*hidden/),
  "the aurora's parent (.hero) must be position: relative with overflow: hidden",
)

// Comments that were corrected must stay corrected.
const tokensHeader = read('app/tokens.css').match(/^\s*\/\*[\s\S]*?\*\//)?.[0] ?? ''
assert(!/3\.13:1/.test(tokensHeader) && /3\.15:1/.test(tokensHeader), 'tokens.css must quote 3.15:1 for brand on cream')
assert(!/\b348\b|\b150\b/.test(tokensHeader), 'tokens.css must not quote the design-target hues')
assert(!/suffix every enzyme/.test(read('components/Mark.tsx')), 'Mark.tsx must not say every enzyme name ends in -ase')

if (failures.length) {
  console.error(`check-furniture: ${failures.length} failure(s)`)
  for (const f of failures) console.error(`  FAIL: ${f}`)
  process.exit(1)
}
console.log('check-furniture: ok')
