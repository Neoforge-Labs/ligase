/**
 * Fail the build when the design tokens stop meeting WCAG AA or when the signal
 * colours drift too close together on the colour wheel.
 *
 * The palette lives in app/tokens.css as `--lg-*` custom properties, once for
 * the light theme (`:root`) and once for the dark theme (`.dark`). Nothing
 * else in the site is allowed to pick a colour, so checking the tokens checks
 * the site. This reads the CSS itself rather than a copy of the values, so the
 * gate cannot go stale.
 *
 * Two things are asserted for each theme:
 *
 *   contrast    every colour used as normal-size text reaches 4.5:1 (WCAG 2.1
 *               AA) against every surface it can sit on, and each signal colour
 *               reaches 4.5:1 against its own -soft tint (the callout fill).
 *   separation  `brand` stays at least 25 degrees of hue away from `notice` and
 *               from `refuse`. Brand-versus-notice is the tightest pair in any
 *               orange-led palette; this is what stops it regressing until the
 *               two read as one colour.
 *
 * `rule` and `rule-strong` are exempt. They are hairlines, not text, and their
 * whole job is to be quiet. They are still parsed, and `contrast()` still
 * handles their alpha channel, because a translucent rule is a legitimate way
 * to write one.
 *
 * Usage:
 *   node scripts/check-contrast.mjs               check app/tokens.css
 *   node scripts/check-contrast.mjs path/to.css   check another file
 *   node scripts/check-contrast.mjs --self-test   verify the maths, then stop
 *
 * The self-test also runs before every real check: a checker whose arithmetic
 * is wrong would pass or fail the palette for the wrong reasons.
 */

import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const TOKENS_CSS = fileURLToPath(new URL('../app/tokens.css', import.meta.url))

/** Used as normal-size text, so all need 4.5:1. */
const TEXT_TOKENS = ['ink', 'ink-muted', 'ink-faint', 'brand', 'refuse', 'agree', 'notice']
const SURFACES = ['paper', 'paper-raised', 'paper-sunken']
const SIGNALS = ['brand', 'refuse', 'agree', 'notice']
const AA_TEXT = 4.5
const MIN_SEPARATION = 25
const SEPARATED_FROM_BRAND = ['notice', 'refuse']

// --- parsing ---------------------------------------------------------------

/** Declarations of `--lg-*` custom properties inside the block opened by `selector`. */
function declarationsOf(css, selector) {
  const opener = new RegExp(`(?:^|[}\\s,])${selector.replace('.', '\\.')}\\s*\\{`, 'm')
  const match = opener.exec(css)
  if (!match) return {}
  const start = match.index + match[0].length
  const end = css.indexOf('}', start)
  const body = css.slice(start, end === -1 ? undefined : end)
  const found = {}
  for (const decl of body.matchAll(/--lg-([a-z0-9-]+)\s*:\s*([^;]+);/g)) {
    found[decl[1]] = decl[2].trim()
  }
  return found
}

/**
 * Read the light (`:root`) and dark (`.dark`) tokens out of the stylesheet.
 * Keys have the `--lg-` prefix removed. The dark theme is returned as written,
 * not merged over the light one, so a token it forgets to redefine is reported
 * rather than silently inherited.
 */
export function parseTokens(css) {
  const bare = css.replace(/\/\*[\s\S]*?\*\//g, '')
  return { light: declarationsOf(bare, ':root'), dark: declarationsOf(bare, '.dark') }
}

// --- colour maths ----------------------------------------------------------

/** `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa` or `rgb()/rgba()` -> channels 0-255 and alpha 0-1. */
export function parseColor(value) {
  const v = String(value).trim()
  const hex = /^#([0-9a-f]{3,4}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(v)
  if (hex) {
    let digits = hex[1]
    if (digits.length <= 4) digits = [...digits].map((d) => d + d).join('')
    const [r, g, b, a] = [0, 2, 4, 6].map((i) =>
      i < digits.length ? parseInt(digits.slice(i, i + 2), 16) : 255,
    )
    return { r, g, b, a: a / 255 }
  }
  const fn = /^rgba?\(\s*([^)]+)\)$/i.exec(v)
  if (fn) {
    const parts = fn[1].split(/[\s,/]+/).filter(Boolean)
    if (parts.length === 3 || parts.length === 4) {
      const channel = (p) => (p.endsWith('%') ? (parseFloat(p) / 100) * 255 : parseFloat(p))
      const [r, g, b] = parts.slice(0, 3).map(channel)
      const a = parts.length === 4
        ? parts[3].endsWith('%') ? parseFloat(parts[3]) / 100 : parseFloat(parts[3])
        : 1
      if ([r, g, b, a].every(Number.isFinite)) return { r, g, b, a }
    }
  }
  throw new Error(`not a colour: ${value}`)
}

/** sRGB channel, 0-1 -> linear light (WCAG 2.1 relative luminance). */
export function srgbToLinear(c) {
  return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
}

function luminanceOf({ r, g, b }) {
  return (
    0.2126 * srgbToLinear(r / 255) +
    0.7152 * srgbToLinear(g / 255) +
    0.0722 * srgbToLinear(b / 255)
  )
}

/** Relative luminance of an opaque colour. Any alpha is ignored; see `contrast`. */
export function relativeLuminance(hex) {
  return luminanceOf(parseColor(hex))
}

/** `fg` laid over an opaque `bg`, blended per channel in sRGB as a browser does. */
function composite(fg, bg) {
  const mix = (f, b) => f * fg.a + b * (1 - fg.a)
  return { r: mix(fg.r, bg.r), g: mix(fg.g, bg.g), b: mix(fg.b, bg.b), a: 1 }
}

/**
 * WCAG 2.1 contrast ratio of `fg` on `bg`, from 1 to 21.
 *
 * A translucent foreground (8-digit hex, rgba()) is composited over `bg`
 * first: it has no luminance of its own, only the one it produces on a
 * surface. The background must be opaque; there is nothing underneath it to
 * composite against.
 */
export function contrast(fg, bg) {
  const back = parseColor(bg)
  if (back.a !== 1) throw new Error(`background must be opaque: ${bg}`)
  const front = composite(parseColor(fg), back)
  const a = luminanceOf(front)
  const b = luminanceOf(back)
  return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05)
}

/** HSL hue of a colour in degrees, [0, 360). Greys have no hue and give 0. */
export function hueOf(hex) {
  const { r, g, b } = parseColor(hex)
  const [R, G, B] = [r / 255, g / 255, b / 255]
  const max = Math.max(R, G, B)
  const delta = max - Math.min(R, G, B)
  if (delta === 0) return 0
  let h
  if (max === R) h = ((G - B) / delta) % 6
  else if (max === G) h = (B - R) / delta + 2
  else h = (R - G) / delta + 4
  return (h * 60 + 360) % 360
}

/** Smallest angle between two hues, 0-180: 350 and 10 are 20 apart, not 340. */
export function separation(a, b) {
  const d = Math.abs(hueOf(a) - hueOf(b)) % 360
  return d > 180 ? 360 - d : d
}

// --- the assertions --------------------------------------------------------

/**
 * Check one theme's tokens. `mode` is 'light' or 'dark' and is only a label on
 * the violations. Returns every violation; an empty array means the theme passes.
 */
export function check(tokens, mode) {
  const violations = []
  const missing = new Set()

  const need = (name) => {
    if (tokens[name] === undefined) {
      if (!missing.has(name)) {
        missing.add(name)
        violations.push({ mode, pair: `--lg-${name}`, kind: 'missing', detail: 'token is not defined' })
      }
      return undefined
    }
    return tokens[name]
  }

  const text = (fgName, bgName) => {
    const fg = need(fgName)
    const bg = need(bgName)
    if (fg === undefined || bg === undefined) return
    try {
      const ratio = contrast(fg, bg)
      if (ratio < AA_TEXT) {
        violations.push({ mode, pair: `${fgName} on ${bgName}`, kind: 'contrast', ratio, required: AA_TEXT, fg, bg })
      }
    } catch (error) {
      violations.push({ mode, pair: `${fgName} on ${bgName}`, kind: 'unparseable', detail: error.message })
    }
  }

  for (const name of TEXT_TOKENS) for (const surface of SURFACES) text(name, surface)
  for (const name of SIGNALS) text(name, `${name}-soft`)

  for (const other of SEPARATED_FROM_BRAND) {
    const brand = need('brand')
    const rival = need(other)
    if (brand === undefined || rival === undefined) continue
    try {
      const angle = separation(brand, rival)
      if (angle < MIN_SEPARATION) {
        violations.push({ mode, pair: `hue of brand vs ${other}`, kind: 'separation', ratio: angle, required: MIN_SEPARATION, fg: brand, bg: rival })
      }
    } catch (error) {
      violations.push({ mode, pair: `hue of brand vs ${other}`, kind: 'unparseable', detail: error.message })
    }
  }

  return violations
}

// --- self-test -------------------------------------------------------------

/*
 * Expected ratios, each confirmed against an independent implementation of the
 * WCAG 2.1 formula. The alpha case composites in sRGB, as a browser paints it:
 * 0x1a/255 of white over #0A0A0B is rgb(35, 35, 36), giving 1.26.
 */
const CASES = [
  { fg: '#E36414', bg: '#F5F5F0', expect: 3.15, label: 'brand orange on cream fails AA' },
  { fg: '#E36414', bg: '#0A0A0B', expect: 5.75, label: 'brand orange on near-black passes AA' },
  { fg: '#F5F5F0', bg: '#0A0A0B', expect: 18.1, label: 'cream on near-black' },
  { fg: '#ffffff1a', bg: '#0A0A0B', expect: 1.26, label: '8-digit hex composites over its surface' },
]

function selfTest() {
  const failures = []
  const assert = (ok, label, detail) => {
    if (!ok) failures.push(`${label}: ${detail}`)
  }

  for (const c of CASES) {
    let got
    try {
      got = contrast(c.fg, c.bg)
    } catch (error) {
      failures.push(`${c.label}: threw ${error.message}`)
      continue
    }
    assert(Math.abs(got - c.expect) < 0.05, c.label, `${c.fg} on ${c.bg} gave ${got.toFixed(3)}, expected ${c.expect}`)
  }

  assert(Math.abs(hueOf('#E36414') - 23) <= 1, 'hueOf brand orange', `gave ${hueOf('#E36414').toFixed(2)}, expected 23 +/- 1`)
  assert(separation('#E36414', '#d9bf4e') >= 25, 'separation brand/notice', `gave ${separation('#E36414', '#d9bf4e').toFixed(2)}, expected >= 25`)
  assert(Math.abs(separation('#ff002b', '#ff2b00') - separation('#ff2b00', '#ff002b')) < 1e-9, 'separation is symmetric', 'a,b and b,a differ')
  assert(Math.abs(separation('#ff002b', '#ff2b00') - 20) < 1, 'separation wraps the wheel', `350 and 10 should be 20 apart, gave ${separation('#ff002b', '#ff2b00').toFixed(2)}`)
  assert(Math.abs(contrast('#000', '#fff') - 21) < 1e-9, 'black on white', `gave ${contrast('#000', '#fff')}, expected 21`)
  assert(Math.abs(contrast('#fff', '#fff') - 1) < 1e-9, 'identical colours', 'expected exactly 1')
  assert(Math.abs(contrast('rgba(255,255,255,0.1)', '#0A0A0B') - contrast('#ffffff1a', '#0A0A0B')) < 0.02, 'rgba() agrees with 8-digit hex', 'differ by more than rounding of the alpha byte')

  const css = ':root{--lg-a:#fff;/* --lg-b:#000; */}.dark{--lg-a:#000;--lg-c:red;}'
  const parsed = parseTokens(css)
  assert(parsed.light.a === '#fff' && !('b' in parsed.light), 'parseTokens light', JSON.stringify(parsed.light))
  assert(parsed.dark.a === '#000' && parsed.dark.c === 'red', 'parseTokens dark', JSON.stringify(parsed.dark))

  return failures
}

// --- main ------------------------------------------------------------------

function describe(v) {
  if (v.kind === 'contrast') {
    return `${v.pair.padEnd(28)} ${v.ratio.toFixed(2)}:1, needs ${v.required}:1   (${v.fg} on ${v.bg})`
  }
  if (v.kind === 'separation') {
    return `${v.pair.padEnd(28)} ${v.ratio.toFixed(1)} deg, needs ${v.required}   (${v.fg} vs ${v.bg})`
  }
  return `${v.pair.padEnd(28)} ${v.detail}`
}

function main(args) {
  const failures = selfTest()
  if (failures.length) {
    console.error(`\ncontrast self-test failed (${failures.length}):\n`)
    for (const failure of failures) console.error(`  ${failure}`)
    console.error('')
    process.exit(1)
  }
  if (args.includes('--self-test')) {
    console.log(`contrast self-test ok: ${CASES.length} ratios, hue, separation, parsing`)
    return
  }

  const file = args.find((a) => !a.startsWith('--')) ?? TOKENS_CSS
  const { light, dark } = parseTokens(readFileSync(file, 'utf8'))
  const violations = [...check(light, 'light'), ...check(dark, 'dark')]

  if (violations.length) {
    console.error(`\n${violations.length} contrast problem(s) in ${file}:\n`)
    for (const mode of ['light', 'dark']) {
      const mine = violations.filter((v) => v.mode === mode)
      if (!mine.length) continue
      console.error(`  ${mode}`)
      for (const v of mine) console.error(`    ${describe(v)}`)
    }
    console.error('')
    process.exit(1)
  }

  const pairs = (TEXT_TOKENS.length * SURFACES.length + SIGNALS.length + SEPARATED_FROM_BRAND.length) * 2
  console.log(`contrast ok: ${pairs} checks across light and dark`)
}

if (process.argv[1] && fileURLToPath(import.meta.url) === process.argv[1]) {
  main(process.argv.slice(2))
}
