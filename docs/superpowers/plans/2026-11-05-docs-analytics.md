# Docs Analytics (Google Tag Manager) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Load GTM container `GTM-KBSX44GX` on every page of the exported
docs site, with Consent Mode defaulting to denied, and count soft
navigations — so GA4 configured in the GTM UI can report acquisition,
volume and behaviour.

**Architecture:** Two inline scripts plus a `<noscript>` iframe at the top
of `<body>` in the root layout (consent defaults first, container loader
second), and a client component that pushes a pageview to `dataLayer` on
App Router route change. `components/Analytics.tsx` owns the container ID
and both pieces; `app/layout.tsx` imports from it. A new assertion in
`check-furniture.mjs --built` fails the build if the tag stops shipping.

**Tech Stack:** Next.js 15 App Router (static export), Nextra 4, React 19,
TypeScript. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-11-05-ligase-docs-analytics-design.md`

## Global Constraints

- **No new npm dependency.** `check-assets.mjs` fails when `package.json`
  dependencies differ from the merge-base with `main`. No
  `@next/third-parties`.
- **Container ID is `GTM-KBSX44GX`**, written in source, declared exactly
  once, in `web/components/Analytics.tsx`.
- **`web/lib/` stays purely generated.** Every file there is gitignored;
  add nothing to it.
- **Production builds only.** The tag is emitted when
  `process.env.NODE_ENV === 'production'`. `next dev` must send nothing.
- **Consent Mode v2, all four signals denied by default**:
  `ad_storage`, `ad_user_data`, `ad_personalization`, `analytics_storage`.
  No banner, no privacy page, no consent-granting UI.
- **No AI attribution** in commits, code or docs (CLAUDE.md).
- There is no test runner in `web/`. The gate is `npm run check`; the
  checker assertion is the test, and `npm run selectors` runs the
  `--built` pass alone.
- **Do not configure GA4 in the GTM UI until Task 3 is committed.** Local
  verification in Task 2 fires real hits at the container; with no GA4 tag
  in it yet, nothing is recorded anywhere.

## Review Focus

Five failure modes the spec implies that no task's deliverable exercises
by default. Each gets a test in the task that owns the code.

1. **`window.dataLayer` undefined when the route-change push runs** — the
   component can mount before the inline snippet has run on a cold cache,
   and `dataLayer.push` on `undefined` throws, losing every later
   pageview silently. Task 2.
2. **The first path counted twice** — the container's own initialisation
   reports the landing page, and a naive `useEffect` on `usePathname()`
   reports it again. Task 2.
3. **Anchor and TOC navigation firing spurious pageviews** — every page
   has an "On this page" rail; clicking it must not look like a visit.
   Task 2.
4. **The snippet emitted more than once per page** — a second copy, from
   a layout or a page, double-counts everything. Task 1.
5. **Paths disagreeing with `trailingSlash: true`** — the site serves
   `/concepts/caching/`; a pageview reporting `/concepts/caching` splits
   one page into two rows in GA4. Task 2.

---

### Task 1: The tag, and the gate that proves it shipped

**Files:**
- Create: `web/components/Analytics.tsx`
- Modify: `web/app/layout.tsx`
- Modify: `web/scripts/check-furniture.mjs` (the `--built` block, which
  begins at `if (process.argv.includes('--built'))`)

**Interfaces:**
- Produces, from `web/components/Analytics.tsx`:
  - `export const GTM_ID = 'GTM-KBSX44GX'`
  - `export function GoogleTagManager(): ReactNode | null` — renders both
    inline scripts and the `<noscript>` iframe, or `null` outside
    production
  - The two script bodies are module constants, not exports: only the
    checker reads them, and it reads them from the file's text.
- Task 2 adds `Analytics` to the same file and relies on `GTM_ID`.

- [ ] **Step 1: Write the failing assertion**

In the `--built` block of `web/scripts/check-furniture.mjs`, after the
existing `theme-color` loop, read the container ID out of
`components/Analytics.tsx` rather than repeating it, strip the RSC flight
payload (verified: Next echoes inline script text into
`self.__next_f.push(...)`, so JSON-LD already appears twice per built page,
in an order that is not the DOM's), then assert across every page in
`pages`:

```js
// Analytics. The container is the only third-party script on the site, and
// analytics that quietly stopped reporting is the failure shape this project
// exists to refuse — so the build asserts the tag shipped, in one piece and
// in the right order, rather than trusting that it did.
let gtmId = ''
try {
  gtmId = /GTM_ID\s*=\s*'([^']+)'/.exec(readFileSync(join(root, 'components/Analytics.tsx'), 'utf8'))?.[1] ?? ''
} catch {}
if (!gtmId) miss.push('components/Analytics.tsx does not declare GTM_ID')
else {
  for (const raw of pages) {
    // Next echoes every inline script's text into the RSC flight payload, so
    // each one appears twice in the HTML and in an order that is not the DOM's.
    // Assert against the real document only.
    const html = raw.replace(/<script[^>]*>self\.__next_f\.push\([\s\S]*?<\/script>/g, '')
    const consentAt = html.indexOf("'consent','default'")
    const loaderAt = html.indexOf('gtm.start')
    if (consentAt === -1) { miss.push('no consent default in the built HTML'); break }
    if (loaderAt === -1) { miss.push(`no ${gtmId} loader in the built HTML`); break }
    if (consentAt > loaderAt) { miss.push('the consent default must come before the container loader'); break }
    if (!html.includes(`'${gtmId}'`)) { miss.push(`the loader does not pass ${gtmId}`); break }
    if (!html.includes(`ns.html?id=${gtmId}`)) { miss.push('the <noscript> fallback is missing'); break }
    if (html.split('gtm.start').length - 1 !== 1) { miss.push('the container loader appears more than once in the document'); break }
    for (const signal of ['ad_storage', 'ad_user_data', 'ad_personalization', 'analytics_storage']) {
      if (!new RegExp(`${signal}:'denied'`).test(html)) { miss.push(`${signal} is not defaulted to denied`); break }
    }
  }
}
```

The duplicate check covers Review Focus 4.

- [ ] **Step 2: Run it to make sure it fails**

Run: `cd web && npm run selectors`
Expected: FAIL, `components/Analytics.tsx does not declare GTM_ID`.
(`out/` is a stale build with no tag; that is what makes this a real gate.)

- [ ] **Step 3: Write `web/components/Analytics.tsx`**

Declare `GTM_ID`, and the two script bodies as module constants so the
ordering is readable and the checker has one place to look.
`CONSENT_DEFAULT_JS` initialises `window.dataLayer`, defines `gtag`, and
calls `gtag('consent','default',{…})` with all four signals `'denied'`.
`GTM_LOADER_JS` is the canonical GTM IIFE — the one that pushes
`{'gtm.start': new Date().getTime(), event: 'gtm.js'}` and inserts the
`gtm.js?id=` script tag. Not the hybrid from the original request, which
pushed neither `gtm.start` nor a `<noscript>` fallback.

`GoogleTagManager()` returns `null` unless
`process.env.NODE_ENV === 'production'`; otherwise a fragment holding both
`<script dangerouslySetInnerHTML>` blocks in order, then the `<noscript>`
iframe pointing at `https://www.googletagmanager.com/ns.html?id=${GTM_ID}`
sized 0×0 with `display:none;visibility:hidden`.

Keep the single quotes in the emitted JavaScript: the Step 1 assertions
match on `'consent','default'` and `ad_storage:'denied'`.

- [ ] **Step 4: Render it in `web/app/layout.tsx`**

Import `GoogleTagManager` and render it as the first child of `<body>`,
above the existing JSON-LD `<script>`. Nothing goes in `<Head>` — the
`theme-color` assertion only inspects `<meta>` tags, so scripts are free,
and consent has to run before anything else analytics-related.

- [ ] **Step 5: Rebuild and run the assertion to verify it passes**

Run: `cd web && npm run build && npm run selectors`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/components/Analytics.tsx web/app/layout.tsx web/scripts/check-furniture.mjs
git commit -m "feat(web): load the GTM container with consent defaulting to denied"
```

---

### Task 2: Count soft navigations

**Files:**
- Create: `web/components/AnalyticsPageview.tsx`
- Modify: `web/app/layout.tsx`
- Modify: `web/scripts/check-furniture.mjs` (the source pass, near the
  existing `Masthead`/`Hero` source assertions around line 608; plus four
  carried fixes in the `--built` block, listed in Step 7)

**Interfaces:**
- Consumes: nothing from Task 1. The bridge pushes to `window.dataLayer`,
  which needs no container ID, so the two modules stay independent.
- Produces: `export function AnalyticsPageview(): null` — a `'use client'`
  component that pushes one `page_view` per route change after the first.

**Why a second file** (controller ruling, 2026-11-05): `'use client'` is
module-scoped in the App Router, so putting the hook in
`components/Analytics.tsx` would make `GoogleTagManager` a client component
too — muddying the server/client boundary, putting the inline scripts'
server rendering at risk, and turning `GTM_ID` into a client reference for
any server importer. `Analytics.tsx` stays a server module and is not
touched by this task.

- [ ] **Step 1: Write the failing assertions**

Nextra navigates softly, so a React hook's behaviour cannot be reached
from the exported HTML. Pin the source shape in the source pass of
`check-furniture.mjs`, alongside the existing `Masthead.tsx` assertions:

```js
// The route-change bridge. Each of these stands for a way the pageview count
// goes wrong in a way nobody would notice: a push against an undefined
// dataLayer throws and loses every later pageview; counting the first path
// double-counts the landing page, which is most of the traffic; and a path
// without the trailing slash the export serves splits one page into two rows.
const analytics = read('components/AnalyticsPageview.tsx')
assert(/^'use client'/m.test(analytics), 'AnalyticsPageview.tsx must be a client component')
assert(/window\.dataLayer\s*=\s*window\.dataLayer\s*\|\|\s*\[\]/.test(analytics), 'the route-change push must initialise dataLayer before using it')
assert(/useRef/.test(analytics), 'the first path is reported by the container, so the bridge needs a ref to skip it')
assert(/usePathname/.test(analytics), 'the bridge must key off usePathname, which ignores hash and query')
assert(/endsWith\('\/'\)/.test(analytics), "the reported path must carry the trailing slash the export serves")
assert(/<AnalyticsPageview\s*\/>/.test(read('app/layout.tsx')), 'layout must render <AnalyticsPageview />')
```

These cover Review Focus 1, 2, 3 and 5.

- [ ] **Step 2: Run them to make sure they fail**

Run: `cd web && npm run furniture`
Expected: FAIL, `AnalyticsPageview.tsx must be a client component`.

- [ ] **Step 3: Implement `AnalyticsPageview(): null` in `web/components/AnalyticsPageview.tsx`**

A `'use client'` component. `usePathname()` for the path — it excludes the
hash and the query, so TOC clicks do not fire (Review Focus 3). A
`useRef` seeded `true` that is flipped on first run, so the initial path
is skipped: the container already reported it (Review Focus 2). Inside
the effect, `window.dataLayer = window.dataLayer || []` before pushing, so
a cold-cache mount cannot throw (Review Focus 1). Normalise the path to
end in `/` to match `trailingSlash: true` (Review Focus 5). Push
`{ event: 'page_view', page_path: <path> }`. Returns `null`.

- [ ] **Step 4: Render it in `web/app/layout.tsx`**

Alongside `<GoogleTagManager />`.

- [ ] **Step 5: Run the assertions to verify they pass**

Run: `cd web && npm run furniture`
Expected: PASS.

- [ ] **Step 6: Verify the real behaviour in a browser**

The static assertions prove the shape, not that a pageview arrives. Build
and serve the export, then drive it:

```bash
cd web && npm run build && python3 -m http.server 842 --directory out
```

In the browser pane at `http://localhost:842/`, confirm in order:
1. On load, `window.dataLayer` exists and holds a `gtm.start` entry, and a
   `consent` entry appears before it.
2. `dataLayer` holds no `page_view` yet — the landing page is not
   double-counted.
3. Click a sidebar link. One `page_view` appears, its `page_path` ending
   in `/`.
4. Click an "On this page" anchor. No new `page_view`.

Stop the server afterwards.

- [ ] **Step 7: Carry the four checker fixes from Task 1's review**

In the `--built` block `check-furniture.mjs` already has, all four were
raised as Minor against Task 1 and are cheapest to fix while this task is
already in the file:

1. The stub skip is a bare substring test, so a future page that merely
   documents redirect stubs would silently escape the gate. Anchor it to
   the stub's own marker, the pattern `check-links.mjs` already uses:
   `/<html[^>]*\bdata-redirect-to="/`.
2. The denied-signal loop tests the whole document, so prose containing
   `ad_storage:'denied'` could mask a missing default. Narrow it to the
   consent script's own text.
3. The `break` inside the signal loop leaves only that inner loop while
   every other check breaks the page loop. Make it consistent.
4. Delete the stray extra blank line after the block, and soften the
   header comment in `Analytics.tsx` that claims the checker asserts the
   noscript's *position* — it asserts presence.

- [ ] **Step 8: Re-verify and commit**

Run: `cd web && npm run furniture && npm run selectors`
Expected: PASS, both.

```bash
git add web/components/AnalyticsPageview.tsx web/components/Analytics.tsx web/app/layout.tsx web/scripts/check-furniture.mjs
git commit -m "feat(web): count soft navigations as pageviews"
```

---

### Task 3: Documentation

**Files:**
- Create: `web/content/project/analytics.mdx`
- Modify: `web/content/project/_meta.ts`
- Modify: `web/content/project/layout.mdx`
- Modify: `CHANGELOG.md`

**Interfaces:** none; this task ships prose.

- [ ] **Step 1: Write `web/content/project/analytics.mdx`**

Frontmatter `title` and `description`, matching the sibling pages in
`web/content/project/`. It goes under `project/` because it is written
about the project, not for somebody using Ligase — CLAUDE.md's
concepts/guide/architecture trio documents engine features, and this is
not one. Cover, in the house voice: what loads and in what order; why
Consent Mode defaults to denied and what that costs (no returning-visitor
identity, and a GA4 "users" figure that leans on modelling at low
traffic, while sessions and pageviews stay real counts); why the
container ID is in source rather than an environment variable; why soft
navigations need the bridge; and that `check-furniture --built` fails if
the tag stops shipping.

It must also carry the trigger hazard found during Task 2, because getting
it wrong silently inflates every number on the dashboard: the container
fires `gtm.historyChange-v2` of its own accord on each soft navigation
**and** on TOC hash changes. A GA4 tag must therefore trigger on the
explicit `page_view` this site pushes, and on nothing else. Adding a
History Change trigger as well would double-count every soft navigation
and count each "On this page" click as a visit. This is the concrete
reason the bridge exists rather than leaning on the built-in trigger, and
`AnalyticsPageview.tsx` is named in the page so a reader can find it.

- [ ] **Step 2: Add it to the section nav and link it**

In `web/content/project/_meta.ts`, add `analytics: 'Analytics'`. Place it
after `releasing` — the existing comment describes the order as what a
new contributor wants, and this is the last thing they need.

In `web/content/project/layout.mdx`, link to `/project/analytics/` from
wherever the site's own `web/` tree is described, so `check-links.mjs`
does not report an orphan.

- [ ] **Step 3: Carry the three fixes from Task 2's review**

All three were raised as Minor against Task 2. The first is promoted by
controller ruling because a gate that cannot fail is the failure shape
this project exists to refuse:

1. `web/scripts/check-furniture.mjs` — the source-pass assertions added in
   Task 2 run against raw source, so the `usePathname` check passes on the
   file's own header comment even if the hook call were deleted. Run those
   assertions against comment-stripped source; the file already has a
   `stripComments` helper used for CSS. Also tighten the `useRef` assertion
   to `/useRef\(true\)/`, so it checks the first-path skip is actually
   seeded rather than that the word appears. Verify by deleting the hook
   call locally, confirming `npm run furniture` now fails, and restoring it.
2. `web/components/Analytics.tsx` — the comment rewrapped in Task 2 left an
   over-long line ("...is not checked). A tag that silently stopped
   shipping is"). Reflow that paragraph to the file's wrap width.
3. `web/components/AnalyticsPageview.tsx` — under React StrictMode in
   `next dev` the effect double-invokes on mount, so the second run pushes
   a `page_view` for the landing path into the local array. Harmless, since
   dev loads no container, but it makes the "first path is skipped" comment
   not strictly true in dev. Add a one-line note saying so.

- [ ] **Step 4: Add the CHANGELOG entry**

Under `## [Unreleased]`, in an `### Added` section, following the house
style of the existing entries: bold one-sentence lead, then what it does
and the trade-off accepted.

- [ ] **Step 5: Run the whole gate**

Run: `cd web && npm run check`
Expected: PASS, every stage.

If `orphans` or `links` objects to the new page, the Step 2 link is
wrong — fix the link, not the checker.

- [ ] **Step 6: Commit**

```bash
git add web/content/project/analytics.mdx web/content/project/_meta.ts web/content/project/layout.mdx web/scripts/check-furniture.mjs web/components/Analytics.tsx web/components/AnalyticsPageview.tsx CHANGELOG.md
git commit -m "docs(web): how the docs site is measured"
```

---

## After the plan

The container is loading but collects nothing until GA4 exists inside it.
That is a GTM UI task, not a repository one: create the GA4 property, add
a GA4 configuration tag on Initialization, and a GA4 event tag on the
custom `page_view` event this plan pushes. Acquisition, volume and page
paths follow from that; scroll depth and outbound clicks to GitHub are
addable in the same UI with no further code.

**Trigger on `page_view` only.** The container also fires
`gtm.historyChange-v2` by itself, on soft navigations and on TOC hash
changes alike. A History Change trigger alongside the `page_view` one
would double-count every in-site navigation and turn every "On this page"
click into a visit — inflating exactly the numbers this work exists to
get right.
