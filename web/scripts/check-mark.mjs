/**
 * Fail when the Ligase mark's two SVGs stop meeting the spec.
 *
 * The mark is a ligature binding the `l` and `i` of "Ligase": a tall serif
 * stem, a shorter stem with its tittle, and one slanted orange tie across
 * both. It is drawn twice. `icon.svg` is the display variant; `favicon.svg`
 * is the small variant, with thicker slabs and tie so it survives 16px.
 *
 * The defect this exists to catch is a favicon that vanishes on a light tab
 * strip, so the favicon's own colour scheme handling is asserted first.
 *
 * Usage:
 *   node scripts/check-mark.mjs
 */
import { readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const read = (p) => readFileSync(join(root, p), 'utf8')

const failures = []
function assert(cond, message) {
  if (!cond) failures.push(message)
}

let favicon = ''
let icon = ''
let mark = ''
let layout = ''
try {
  favicon = read('public/favicon.svg')
  icon = read('public/icon.svg')
} catch (err) {
  console.error(`FAIL: ${err.message}`)
  process.exit(1)
}
mark = read('components/Mark.tsx')
layout = read('app/layout.tsx')

// favicon.svg must carry BOTH tab-strip schemes
assert(/prefers-color-scheme:\s*dark/.test(favicon), 'favicon has no dark-scheme override')
assert(/#0A0A0B/i.test(favicon), 'light tab strip needs dark letterforms')
assert(/#F5F5F0/i.test(favicon), 'dark tab strip needs cream letterforms')
// the default (outside the media query) must be the dark one
const outsideStyle = favicon.replace(/<style>[\s\S]*?<\/style>/, '')
assert(
  /#0A0A0B/i.test(outsideStyle) && !/#F5F5F0/i.test(outsideStyle),
  'favicon letterforms must default to dark with cream only inside the media query',
)

// the tie is slanted, not flat, in both files, and stays orange
assert(/<polygon/.test(favicon) && /<polygon/.test(icon), 'tie must be a <polygon>')
for (const [name, svg] of [['favicon', favicon], ['icon', icon]]) {
  const tie = svg.match(/<polygon[^>]*>/)?.[0] ?? ''
  assert(/#E36414/i.test(tie), `${name} tie must be #E36414`)
}

// small variant is the thicker one: slabs are 6 units, the display variant's 3.5
assert(/height="6"/.test(favicon), 'favicon slabs must be 6 units')
assert(/height="3\.5"/.test(icon), 'icon slabs must be 3.5 units')

// exact geometry, transcribed from the brief
const has = (svg, s) => svg.includes(s)
for (const s of [
  '<rect x="30" y="14" width="10" height="68"',
  '<rect x="24" y="14" width="22" height="3.5"',
  '<rect x="22" y="78.5" width="26" height="3.5"',
  '<rect x="62" y="42" width="10" height="40"',
  '<rect x="56" y="42" width="22" height="3.5"',
  '<rect x="54" y="78.5" width="26" height="3.5"',
  '<circle cx="67" cy="28" r="6"',
  'points="30,56 72,48 72,57 30,65"',
]) assert(has(icon, s), `icon.svg missing ${s}`)
for (const s of [
  '<rect x="26" y="10" width="16" height="76"',
  '<rect x="18" y="10" width="32" height="6"',
  '<rect x="15" y="80" width="38" height="6"',
  '<rect x="60" y="44" width="16" height="42"',
  '<rect x="52" y="44" width="32" height="6"',
  '<rect x="49" y="80" width="38" height="6"',
  '<circle cx="68" cy="26" r="9"',
  'points="26,58 76,52 76,66 26,72"',
]) assert(has(favicon, s), `favicon.svg missing ${s}`)
assert(/viewBox="0 0 100 100"/.test(favicon) && /viewBox="0 0 100 100"/.test(icon), 'viewBox must be 0 0 100 100')

// neither file depends on a webfont
assert(!/font-family|<text/.test(favicon + icon), 'SVGs must not use text or fonts')

// the React mark draws the display geometry, with theme-aware colour
assert(/<polygon/.test(mark) && /points="30,56 72,48 72,57 30,65"/.test(mark), 'Mark.tsx must draw the slanted tie')
assert(/currentColor/.test(mark), 'Mark.tsx letterforms must use currentColor')
assert(/--lg-brand/.test(mark + read('components/Mark.module.css')), 'Mark tie must use --lg-brand')
assert(!/nick|double helix/.test(mark), 'Mark.tsx still carries the old docstring')

// the display face is wired to the variable tokens.css already references
assert(/Playfair_Display/.test(layout), 'layout.tsx must load Playfair_Display')
assert(/variable:\s*'--font-playfair'/.test(layout), "Playfair variable must be '--font-playfair'")
assert(/playfair\.variable/.test(layout), 'Playfair .variable must be on <html>')
// metadata.icons.icon must be the SMALL variant only. icon.svg (display) collapses at 16px, and
// listing both lets the browser choose it for the tab strip. Note 'favicon.svg' contains 'icon.svg'
// as a substring, so match on the quoted path.
const iconsBlock = layout.match(/icons:\s*\{[\s\S]*?\n  \},/)?.[0] ?? ''
assert(iconsBlock !== '', 'layout.tsx must declare metadata.icons')
assert(/'\/favicon\.svg'/.test(iconsBlock), 'metadata.icons must reference /favicon.svg')
assert(!/'\/icon\.svg'/.test(iconsBlock), 'metadata.icons must not declare /icon.svg (display variant)')
assert(!/apple/.test(iconsBlock), 'metadata.icons must not set apple; app/apple-icon.tsx owns it')

if (failures.length) {
  for (const f of failures) console.error(`FAIL: ${f}`)
  process.exit(1)
}
console.log('check-mark: ok')
