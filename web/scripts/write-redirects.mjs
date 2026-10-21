#!/usr/bin/env node
/**
 * Write redirect stubs for the URLs the MkDocs site used to serve.
 *
 * Reorganising the docs moved most pages, and ligase.neoforgelabs.tech had
 * been serving the old paths. Eighteen of them started returning 404 the
 * moment the new site went live — every architecture page among them — which
 * breaks anybody's bookmark, anything linking in, and the search results that
 * already point at them.
 *
 * GitHub Pages serves static files and cannot redirect, so each old path gets
 * a small HTML page that does it client-side: a canonical link so search
 * engines move their index across, a meta refresh for the no-JavaScript case,
 * and a script that goes immediately and carries any #fragment over.
 *
 * Every target is checked against the built output. A redirect pointing at a
 * page that does not exist is worse than the 404 it replaces, because it
 * looks deliberate.
 */

import { mkdir, readFile, writeFile, access } from 'node:fs/promises'
import { dirname, join, resolve } from 'node:path'

const OUT = resolve(process.argv[2] ?? 'out')

/** Old MkDocs path -> where that page lives now. */
const MOVED = {
  // The four root-level design docs split: the dimension catalogue is
  // reference material, the rest are about the project.
  'decisions/': '/project/decisions/',
  'dimensions/': '/reference/dimensions/',
  'golden-example/': '/project/golden-example/',
  'milestones/': '/project/milestones/',

  // The API reference joined the other things you look up.
  'api/': '/reference/api/',

  // Internals moved under project/.
  ...Object.fromEntries(
    [
      'cache',
      'canonicalisation',
      'execution',
      'graph',
      'hivdb',
      'provenance',
      'type-system',
      'workspace',
    ].map((page) => [`architecture/${page}/`, `/project/architecture/${page}/`]),
  ),

  // Pages that were filed as concepts or guides but are about the project.
  'concepts/project-layout/': '/project/layout/',
  'concepts/validation/': '/project/validation/',
  'guide/releasing/': '/project/releasing/',

  // Installation split in two; a bare link meant "how do I get it".
  'guide/installation/': '/start/installation/',

  // The notebooks became a section of their own.
  'guide/notebooks/': '/notebooks/',
}

function page(to) {
  return `<!doctype html>
<html lang="en" data-redirect-to="${to}">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Moved — Ligase</title>
<link rel="canonical" href="https://ligase.neoforgelabs.tech${to}">
<meta name="robots" content="noindex">
<meta http-equiv="refresh" content="0; url=${to}">
<script>
  // Immediate, and keeps any #fragment the old link carried.
  location.replace(${JSON.stringify(to)} + location.hash);
</script>
<style>
  body { font-family: ui-sans-serif, system-ui, sans-serif; margin: 4rem auto;
         max-width: 40rem; padding: 0 1.5rem; line-height: 1.6; }
</style>
</head>
<body>
<h1>This page moved</h1>
<p>It now lives at <a href="${to}">${to}</a>.</p>
</body>
</html>
`
}

async function exists(path) {
  try {
    await access(path)
    return true
  } catch {
    return false
  }
}

const problems = []
let written = 0

for (const [from, to] of Object.entries(MOVED)) {
  if (!(await exists(join(OUT, to.replace(/^\/|\/$/g, ''), 'index.html')))) {
    problems.push(`${from} -> ${to}: nothing is built at the target`)
    continue
  }
  const target = join(OUT, from, 'index.html')
  if (await exists(target)) {
    problems.push(`${from}: a real page is built here; it must not be shadowed`)
    continue
  }
  await mkdir(dirname(target), { recursive: true })
  await writeFile(target, page(to), 'utf8')
  written += 1
}

if (problems.length) {
  console.error(`\n${problems.length} redirect problem(s):\n`)
  for (const problem of problems) console.error(`  ${problem}`)
  console.error('')
  process.exit(1)
}

console.log(`redirects: ${written} old URLs point at their new homes`)
