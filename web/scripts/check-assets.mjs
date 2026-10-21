/**
 * Fail when the generated raster assets stop meeting the spec.
 *
 * Three routes produce files at export time: app/opengraph-image.tsx (the
 * social card), app/apple-icon.tsx (the home-screen icon) and
 * app/not-found.tsx (the 404). This checks the build output, so it needs
 * `out/` to exist: run `npm run build` first.
 *
 * Next writes the two images WITHOUT a file extension (`out/opengraph-image`,
 * not `out/opengraph-image.png`), so each is identified by its PNG signature
 * and the width and height in its IHDR chunk rather than by its name.
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
import { existsSync, readFileSync } from 'node:fs'
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

function checkRaster(label, candidates, width, height) {
  const found = candidates.map((c) => join(root, 'out', c)).find((p) => existsSync(p))
  if (!found) {
    failures.push(`${label}: none of ${candidates.map((c) => `out/${c}`).join(', ')} exists (run npm run build)`)
    return
  }
  const size = pngSize(found)
  assert(size !== null, `${label}: ${found} is not a PNG`)
  if (size) {
    assert(
      size.width === width && size.height === height,
      `${label}: expected ${width}x${height}, got ${size.width}x${size.height}`,
    )
  }
}

checkRaster('social card', ['opengraph-image', 'opengraph-image.png', 'opengraph-image/index.png'], 1200, 630)
checkRaster('apple icon', ['apple-icon', 'apple-icon.png', 'apple-icon/index.png'], 180, 180)

// ---- the pages that reference them -----------------------------------------

const readOut = (p) => (existsSync(join(root, 'out', p)) ? readFileSync(join(root, 'out', p), 'utf8') : null)

const home = readOut('index.html')
assert(home !== null, 'out/index.html is missing')
if (home) {
  // Both tags: the previous site set only twitter:card and so had no card image.
  assert(/property="og:image" content="[^"]*\/opengraph-image/.test(home), 'index.html has no og:image pointing at /opengraph-image')
  assert(/name="twitter:image" content="[^"]*\/opengraph-image/.test(home), 'index.html has no twitter:image pointing at /opengraph-image')
  assert(/property="og:image:width" content="1200"/.test(home), 'og:image:width must be 1200')
  assert(/property="og:image:height" content="630"/.test(home), 'og:image:height must be 630')
}

const notFound = readOut('404.html')
assert(notFound !== null, 'out/404.html is missing')
if (notFound) {
  assert(/Nothing is bound to this address/.test(notFound), '404.html is not the refusal page')
  assert(/var\(--lg-refuse\)/.test(notFound), '404.html must draw the broken tie in var(--lg-refuse)')
}

// ---- known gap, reported but not failing -----------------------------------
// Next adds static icon files to <head> only when metadata.icons is unset, and
// layout.tsx sets it, so app/apple-icon.tsx is built but never linked. Fixing
// that means an `apple` entry in layout.tsx, which check-mark.mjs currently
// forbids. Turn this into an assert once that decision is made.
const warnings = []
if (home && !/rel="apple-touch-icon"/.test(home)) {
  warnings.push('index.html has no <link rel="apple-touch-icon">: apple-icon is built but not linked')
}

for (const w of warnings) console.warn(`WARN: ${w}`)
if (failures.length) {
  for (const f of failures) console.error(`FAIL: ${f}`)
  process.exit(1)
}
console.log('check-assets: ok')
