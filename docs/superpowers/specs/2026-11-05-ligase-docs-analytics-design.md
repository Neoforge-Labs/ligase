# Google Tag Manager on the docs site

Date: 2026-11-05
Status: proposed

## Intent and the why

Kelyn wants to know three things about ligase.neoforgelabs.tech: where
visitors came from, how many there are, and what they did once they
arrived. A Google Tag Manager container (`GTM-KBSX44GX`) already exists.

A container on its own collects nothing — it is a tag loader. GA4,
configured inside the container's web UI, is what answers the three
questions. So the repository's job is to load the container correctly and
to tell it when a page was viewed; everything about *what* is measured
stays in the GTM UI, where it can change without a pull request.

## Goals

- The container loads on every page of the exported site.
- Soft (client-side) navigations are counted, not just the first load.
- GA4 can report acquisition source, visit counts and page paths.
- Future measurement — scroll depth, clicks through to GitHub, which
  silent-failure page people land on — is addable in the GTM UI with no
  code change.
- If the tag ever stops shipping, the build says so.

## Non-goals

- A cookie banner, a consent UI, or a privacy page.
- Returning-visitor identity, or any cross-session user stitching.
- Advertising, remarketing or conversion tags.
- Measuring anything in `next dev` or in pull-request builds.

## Anti-goals

Things that would make this a failure even if the tag worked:

- **A silent failure.** This project exists because silent failures are
  the worst kind. Analytics that quietly stopped reporting, or a tag
  gated behind an environment variable nobody set, is exactly the shape
  of bug Ligase is written to refuse. The build must assert the tag
  shipped.
- **Cookies without consent.** GA4's default cookies would put the site
  in breach of GDPR/ePrivacy for EU and UK visitors.
- **A new npm dependency.** `check-assets.mjs` fails the build when
  `package.json` dependencies differ from the merge-base with `main`, and
  CLAUDE.md requires asking first.

## Constraints

- `output: 'export'` to GitHub Pages. No server, no middleware, no CSP
  nonce, no geo detection.
- Next.js App Router with Nextra 4, so navigation between pages is soft:
  one document load, many route changes.
- `npm run check` is the gate. Relevant parts: `check-assets.mjs`
  forbids new dependencies; `check-furniture.mjs --built` asserts the
  first `<meta name="theme-color">` stays unconditional (scripts do not
  affect it); `check-links.mjs` fails on a page nothing links to.

## Approaches

1. **Container in code, GA4 in the GTM UI.** The snippet and a
   route-change bridge live in the repository; tags, triggers and the GA4
   property live in the container. **Chosen.** It is the only option
   where "what did they do" can grow without touching the repository, and
   the container already exists.
2. **GA4 directly via `gtag.js`, no GTM.** About half the third-party
   JavaScript and one less layer, but every new measurement becomes a
   code change and a deploy, and it discards the existing container.
   Rejected.
3. **Cookieless analytics instead** (Cloudflare Web Analytics, Plausible,
   Umami). No cookies, no consent question, cheapest for a pre-alpha
   docs site — but it is not GTM, campaign-level acquisition is thinner,
   and it would mean abandoning work already done. Rejected.

Consent was decided separately: **Consent Mode v2, defaulting to denied,
with no banner.** GA4 then sends cookieless pings, which still carry page
path and referrer, so all three questions are answered at the aggregate
level. The cost, accepted knowingly, is no returning-visitor identity and
a GA4 "users" figure that leans on modelling and will be unreliable at
low traffic. Sessions and pageviews remain real counts.

## The design

Four changes, all in `web/`.

### 1. `app/layout.tsx` — the tag

Three children at the top of `<body>`, before `<Layout>`, following the
pattern the JSON-LD block already uses (inline
`dangerouslySetInnerHTML`, not `next/script`), because the ordering below
is the whole point and inline scripts make it explicit:

1. **Consent defaults**, first. `ad_storage`, `ad_user_data`,
   `ad_personalization` and `analytics_storage` all set to `denied`.
   This must execute before the container loads or the container will
   have already set cookies.
2. **The canonical GTM snippet** for `GTM-KBSX44GX` — the real one, which
   pushes `gtm.start` and injects the script tag. Not the hybrid in the
   original request, which loaded `gtm.js` but then called `gtag('js',
   …)`, pushing neither `gtm.start` nor the `<noscript>` fallback.
3. **The `<noscript>` iframe**, so a visitor with JavaScript off is still
   counted.

All three are emitted only when `process.env.NODE_ENV === 'production'`,
which `next build` sets and `next dev` does not. The container ID is
written in the source, not read from an environment variable: it is a
public identifier rather than a secret, and an unset variable would make
the tag vanish with nothing noticing — the anti-goal above.

### 2. `components/Analytics.tsx` — soft navigations

A client component using `usePathname()` that pushes an explicit
pageview event to `dataLayer` on each route change. It deliberately skips
the first path, which the container's own initialisation already
reports, so the landing page is not counted twice. GTM's built-in History
Change trigger could do this in the UI instead; an explicit push is
chosen because it is visible in the repository and can be asserted by a
checker.

### 3. `scripts/check-furniture.mjs` — the assertion

Added to the `--built` pass: every exported page carries the container
ID, the consent defaults appear before the container loader in document
order, and the `<noscript>` fallback is present. This is what makes the
first anti-goal enforceable rather than aspirational.

### 4. Docs

- `web/content/project/analytics.mdx`, plus its entry in
  `web/content/project/_meta.ts` and a link from
  `web/content/project/layout.mdx` so `check-links.mjs` does not see an
  orphan. It goes under `project/` because it is written about the
  project rather than for somebody using Ligase; the
  concepts/guide/architecture trio in CLAUDE.md's documentation rule
  describes engine features, which this is not.
- `CHANGELOG.md`.

## Left to the builder

- The exact wording of the docs page.
- Whether the route-change push reuses GA4's `page_view` event name or a
  project-specific one, and the shape of its payload.
- How the consent defaults and the loader are spelled, so long as the
  ordering holds and the checker can see both.

## Amendment, 2026-11-05 (after Task 1 review)

§2 above names `components/Analytics.tsx` as the home of the route-change
bridge. It is instead `components/AnalyticsPageview.tsx`, a second file.

`'use client'` is module-scoped in the App Router: one directive would have
made `GoogleTagManager` a client component alongside the hook, muddying the
server/client boundary, putting the inline scripts' server rendering at
risk, and turning `GTM_ID` into a client reference for any server importer.
The split keeps `Analytics.tsx` a server module and costs nothing — the
bridge pushes to `window.dataLayer` and needs no container ID, so the two
modules share no imports.

Nothing else in this design changes: the ordering, the consent defaults, the
production gate and the build assertion are all as written above.

## Correction, 2026-11-05 (after Task 3 review)

The Approaches section above says that under Consent Mode denied "sessions
and pageviews remain real counts". **The sessions half of that is wrong**,
and the design's expectations are corrected here.

With `analytics_storage` denied, GA4 writes no `_ga` cookie and sends
cookieless pings. Those pings carry no client ID, so GA4 cannot tell
whether ten pageviews came from ten readers or from one. Sessions are
stitched from the client ID, so **sessions are as unreliable as users**,
not a real count.

Behavioural modelling does not fill the gap here either. Google's
documented thresholds require a consented cohort to train on — on the order
of a thousand daily events with consent denied for seven days *and* a
thousand daily users with consent granted over seven of the previous
twenty-eight. This site has no banner, so nothing is ever granted, and it
has no traffic of that order. There will be no modelled fill-in, only
unmodelled gaps.

What the setup therefore does and does not answer:

- **Real, from the pings:** counts of `page_view` events, the page path on
  each, and the referrer that brought it. So "where did they come from"
  and "which pages get read" are answerable, as event counts.
- **Not answerable:** unique visitors, sessions, returning visitors, and
  any per-reader journey through the site. "How many" means pageviews, not
  people.

This does not change what gets built — the implementation is the same
either way — but it changes what the docs page may claim and what the
eventual GA4 property can be expected to report. The page must not promise
sessions or users.

## Amendment, 2026-11-05 (the `<noscript>` iframe is removed)

§1.3 above lists **the `<noscript>` iframe** as the third child of `<body>`,
"so a visitor with JavaScript off is still counted". It is removed, from the
component and from the build assertion, and the docs page no longer describes
it.

It cannot do the thing it was justified by. GA4's tags are JavaScript, and
JavaScript does not run inside `ns.html` — the iframe can carry only tags that
are a plain image or iframe request. With GA4 as the only tag this container is
ever intended to hold, a no-JavaScript visitor is uncounted whether the iframe
ships or not. So the measurement it was there to protect never existed.

What it does do is make a request to Google. And the consent default is itself
JavaScript, so for exactly the visitor the iframe was aimed at, the default
never runs: the iframe is the one request on this site that Consent Mode
cannot govern. On a site whose chosen posture is deny-by-default and whose
whole justification for having no banner is that nothing is ever granted, an
ungovernable third-party request that measures nothing is the wrong trade.

Removing it loses no measurement that was ever achievable. The `--built` pass
drops the `ns.html?id=` assertion; everything else it checks — the consent
default before exactly one loader, the ID read out of the source, the four
signals in the consent script's own text — is unchanged.
