/**
 * Fail when the generated raster assets stop meeting the spec.
 *
 * Three routes produce files at export time: app/opengraph-image.tsx (the
 * social card), app/apple-icon.tsx (the home-screen icon) and
 * app/not-found.tsx (the 404). This checks the build output, so it needs
 * `out/` to exist: run `npm run build` first.
 *
 * The two images are route handlers in folders named `*.png`, so they export as
 * `out/opengraph-image.png` and `out/apple-icon.png`. (The file conventions
 * would write them with no extension.) Each is checked by its PNG signature and
 * the width and height in its IHDR chunk, and every reference to them in the
 * emitted HTML must resolve to a file that exists.
 *
 * The dependency check compares package.json with the one at the branch
 * merge-base, not with HEAD: a file compared with itself cannot fail. It is
 * therefore a guard for a feature branch. On the base branch itself it
 * compares main with main and passes trivially. Set LIGASE_BASE_REF to
 * compare against another ref.
 *
 * Usage:
 *   node scripts/check-assets.mjs
 */
import { execFileSync } from 'node:child_process'
import { existsSync, readdirSync, readFileSync } from 'node:fs'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

const root = join(dirname(fileURLToPath(import.meta.url)), '..')
const failures = []
function assert(cond, message) {
  if (!cond) failures.push(message)
}

// ---- no dependency was added -----------------------------------------------

function git(...args) {
  return execFileSync('git', args, {
    cwd: root,
    encoding: 'utf8',
    stdio: ['ignore', 'pipe', 'pipe'],
  }).trim()
}

try {
  const baseRef = process.env.LIGASE_BASE_REF || 'main'
  const base = git('merge-base', baseRef, 'HEAD')
  const before = JSON.parse(git('show', `${base}:./package.json`))
  const now = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8'))
  for (const field of ['dependencies', 'devDependencies']) {
    assert(
      JSON.stringify(now[field] ?? {}) === JSON.stringify(before[field] ?? {}),
      `package.json ${field} differ from the merge-base with ${baseRef} (${base.slice(0, 9)}): ` +
        'no dependency may be added without approval',
    )
  }
} catch (err) {
  // Never skip silently: an unchecked constraint is how one gets broken.
  failures.push(
    `cannot compare package.json with the branch merge-base (${String(err.message).split('\n')[0]}); ` +
      'set LIGASE_BASE_REF to a ref that exists locally',
  )
}

// ---- the built rasters -----------------------------------------------------

const PNG_SIGNATURE = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])

/** Width and height from a PNG's IHDR, or null if the file is not a PNG. */
function pngSize(path) {
  const bytes = readFileSync(path)
  if (bytes.length < 24 || !bytes.subarray(0, 8).equals(PNG_SIGNATURE)) return null
  if (bytes.subarray(12, 16).toString('latin1') !== 'IHDR') return null
  return { width: bytes.readUInt32BE(16), height: bytes.readUInt32BE(20) }
}

function checkRaster(file, width, height) {
  const path = join(root, 'out', file)
  if (!existsSync(path)) {
    failures.push(`out/${file} does not exist (run npm run build)`)
    return
  }
  const size = pngSize(path)
  assert(size !== null, `out/${file} is not a PNG`)
  if (size) {
    assert(
      size.width === width && size.height === height,
      `out/${file}: expected ${width}x${height}, got ${size.width}x${size.height}`,
    )
  }
}

checkRaster('opengraph-image.png', 1200, 630)
checkRaster('apple-icon.png', 180, 180)

// GitHub Pages types a response by its extension, and scrapers check
// Content-Type, so an extensionless image is one they may refuse.
for (const bare of ['opengraph-image', 'apple-icon']) {
  assert(!existsSync(join(root, 'out', bare)), `out/${bare} exists without an extension`)
}

// ---- every reference to them -----------------------------------------------

const OUT = join(root, 'out')

function walk(dir) {
  const files = []
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const path = join(dir, entry.name)
    if (entry.isDirectory()) {
      if (entry.name !== '_pagefind') files.push(...walk(path))
    } else if (/\.(html|txt)$/.test(entry.name)) files.push(path)
  }
  return files
}

if (existsSync(OUT)) {
  for (const file of walk(OUT)) {
    // `/opengraph-image` or `/apple-icon` not followed by `.png` is a
    // reference to the extensionless file that no longer exists.
    const text = readFileSync(file, 'utf8')
    const bad = text.match(/\/(?:opengraph-image|apple-icon)(?!\.png)(?=["'?#\s\\)])/g)
    if (bad) {
      failures.push(`${file.slice(OUT.length + 1)} has an extensionless image reference (${bad.length}x)`)
    }
  }
}

const readOut = (p) => (existsSync(join(OUT, p)) ? readFileSync(join(OUT, p), 'utf8') : null)

/** The value of a tag's attribute, or null. */
const attr = (html, tag, key, keyValue, valueName) =>
  html.match(new RegExp(`<${tag}[^>]*${key}="${keyValue}"[^>]*${valueName}="([^"]*)"`))?.[1] ??
  html.match(new RegExp(`<${tag}[^>]*${valueName}="([^"]*)"[^>]*${key}="${keyValue}"`))?.[1] ??
  null

/** A reference must be absolute on the site's own origin and name a file in out/. */
function resolvesToFile(ref) {
  if (ref === null) return false
  const path = ref.replace(/^https:\/\/ligase\.neoforgelabs\.tech/, '').split(/[?#]/)[0]
  return path.startsWith('/') && existsSync(join(OUT, path))
}

for (const page of ['index.html', '404.html', 'concepts/pipelines/index.html']) {
  const html = readOut(page)
  if (html === null) {
    failures.push(`out/${page} is missing`)
    continue
  }
  const og = attr(html, 'meta', 'property', 'og:image', 'content')
  const tw = attr(html, 'meta', 'name', 'twitter:image', 'content')
  const apple = attr(html, 'link', 'rel', 'apple-touch-icon', 'href')
  assert(resolvesToFile(og) && /\.png$/.test(og ?? ''), `${page}: og:image (${og}) does not resolve to a .png in out/`)
  assert(resolvesToFile(tw) && /\.png$/.test(tw ?? ''), `${page}: twitter:image (${tw}) does not resolve to a .png in out/`)
  assert(resolvesToFile(apple) && /\.png$/.test(apple ?? ''), `${page}: apple-touch-icon (${apple}) does not resolve to a .png in out/`)
  assert((html.match(/property="og:image"/g) ?? []).length === 1, `${page}: expected exactly one og:image tag`)
  assert(/property="og:image:width" content="1200"/.test(html), `${page}: og:image:width must be 1200`)
  assert(/property="og:image:height" content="630"/.test(html), `${page}: og:image:height must be 630`)
}

const notFound = readOut('404.html')
if (notFound) {
  assert(/Nothing is bound to this address/.test(notFound), '404.html is not the refusal page')
  assert(/var\(--lg-refuse\)/.test(notFound), '404.html must draw the broken tie in var(--lg-refuse)')
}

if (failures.length) {
  for (const f of failures) console.error(`FAIL: ${f}`)
  process.exit(1)
}
console.log('check-assets: ok')
