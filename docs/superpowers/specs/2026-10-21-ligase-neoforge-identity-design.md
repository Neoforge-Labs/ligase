# Ligase visual identity, and the docs site rebuilt onto it

Date: 2026-10-21
Status: proposed, awaiting approval

## Intent

Ligase has a documentation site with a strong, self-consistent look — warm
paper, violet accent, serif prose — that was designed before it had to belong
to anything. It now has to belong to Neoforge Labs.

neoforgelabs.tech is a research-journal identity: Playfair Display serif
wordmarks in cream `#f5f5f0` on near-black `#0a0a0b`, a single flame-orange
`#e36414` accent, and typographic furniture borrowed from scientific
publishing — `VOL. 01`, `§ 01`, `FIG. 01`, `MMXXV`, letterspaced monospace
eyebrows. It has no icon mark at all. Every sub-brand (NeoFarm, NeoHealth,
NeoForge, NeoRx, NeoRoute) is a serif wordmark over an orange hairline with a
monospace eyebrow.

Two things follow. Ligase needs a mark that is typographic rather than
geometric, because an abstract glyph is a foreign object in this system. And
the docs site needs to be rebuilt onto the Neoforge palette and type, deeply
enough that it reads as a Neoforge artefact rather than a recoloured Nextra
theme.

## Why the mark is a ligature

*Ligase* and *ligature* are the same Latin root, *ligare*, to bind. Neoforge
is already a serif-display brand, and the ligature is the one mark native to
serif typography.

It is also the right idea rather than merely a pun. A ligature exists only for
particular letter pairs — `fi`, `fl`, `ffi` are real, `qz` is not. You cannot
ligate arbitrary glyphs. That is dimension checking, expressed in type: the
mark and the type system are the same idea, which is the test a logo should
pass and almost never does.

## Goals

- A mark that works as a 16px favicon, as a product mark beside the other
  Neoforge sub-brands, and on a portfolio, in that order of difficulty.
- A docs site that a reader arriving from neoforgelabs.tech recognises
  immediately as the same organisation.
- The engine's four signal colours survive as four distinguishable
  semantics. This is load-bearing: Ligase's product is a well-written
  refusal, and a diagnostic that cannot be told apart from a link has lost
  the argument.
- 52,000 words of technical prose remain comfortable to read for an hour.
- `npm run check` passes: regenerate, build, link-check, no orphan pages, no
  dangling anchors.

## Non-goals

- The Rust crates and the Python package. Untouched.
- Prose content and navigation structure. Untouched; this is a repaint and a
  re-set, not a rewrite.
- neoforgelabs.tech itself.
- Replacing Nextra, or restructuring `web/`.
- The dead `site/` directory (old MkDocs output).

## Anti-goals — what would make this a failure even if it shipped

- Diagnostics, passes and notices that stop being colour-distinguishable
  from each other or from brand orange.
- Body prose measurably harder to read than today's. A Didone display face
  set at 19px for an hour is the specific trap.
- A result that looks like the Nextra dark default with orange links. If the
  journal furniture does not land, the brand has not arrived.
- A broken `npm run check`, or generated pages that stop generating.
- Any AI attribution in any commit, page or comment. Cardinal rule.

## Constraints

- Static export (`output: 'export'`, GitHub Pages). No image optimiser, no
  server. Every asset is a file in `web/public/`, which does not exist yet.
- Nextra derives its own accent and page background from the `<Head color>`
  HSL triple and `backgroundColor` in `app/layout.tsx`. Those must change
  there, not be fought with `!important`.
- Five generated artefacts must keep generating untouched: `content/api/`,
  `lib/diagnostics.generated.ts`, `content/notebooks/`,
  `content/reference/cli.mdx`, `lib/quickstart.generated.ts`.
- Everything visual already routes through `--lg-*` tokens in
  `app/tokens.css`. The repaint is mostly that one file. This is the reason
  the job is tractable.
- Shiki is configured with `github-light` / `github-dark-default`. Both will
  look wrong against cream and near-black.

## The mark

A ligature binding the `l` and `i` of Ligase: a tall stem, a short stem with
its tittle, and one horizontal tie in flame orange crossing both.

Construction, on a 100×100 viewBox:

- Tall stem: `x 30, y 14, w 10, h 68`; head serif `x 24, y 14, w 22, h 3.5`;
  foot serif `x 22, y 78.5, w 26, h 3.5`
- Short stem: `x 62, y 42, w 10, h 40`; head serif `x 56, y 42, w 22, h 3.5`;
  foot serif `x 54, y 78.5, w 26, h 3.5`
- Tittle: `cx 67, cy 28, r 6`
- The tie: `x 30, y 54, w 42, h 9`, in brand orange

Serifs are flat slabs rather than bracketed, so the mark holds its shape at
16px where bracketing turns to mush. The tie is the only coloured element at
every size, which is what makes the mark legible as one gesture in a 16px tab
strip.

Letterforms are drawn as rectangles rather than set from Playfair. The mark
must not depend on a webfont, must be identical across rasterisers, and must
survive being inlined into a favicon.

### Variants

| Variant | Letterforms | Tie | Where |
|---|---|---|---|
| Dark | `#f5f5f0` | `#e36414` | Default. Navbar, favicon, OG, products |
| Light | `#0a0a0b` | `#e36414` | Light-mode navbar, cream surfaces |
| Single-colour | `currentColor` | `currentColor` | Stamps, embroidery, one-colour print |

### The refusal state

The same two letterforms with the tie **broken and offset** rather than bound,
in `refuse`. Not the logo — a companion device, for the 404 page and build
failures. It says what Ligase is for in one glyph.

### Lockups

- **Primary**: mark + `Ligase` in Playfair Display 400, cream, mark at cap
  height, gap 0.34em.
- **Sub-brand card**: monospace eyebrow / mark + wordmark / 52px orange
  hairline / one-line description. Matches the NeoFarm–NeoRoute cards
  exactly, so Ligase can drop into that grid.
- **Mark alone**: favicons, app icons, avatars, anywhere under 32px.

## The palette

Dark is the default. Light is kept and fully retuned, because this site is
long-form reading and printing.

### Dark (default)

| Token | Value | Note |
|---|---|---|
| `--lg-paper` | `#0a0a0b` | Neoforge `--background`, unchanged |
| `--lg-paper-sunken` | `#121216` | inline code, console blocks |
| `--lg-paper-raised` | `#17171c` | fenced blocks, cards |
| `--lg-ink` | `#f5f5f0` | Neoforge `--foreground`, warm cream |
| `--lg-ink-muted` | `#a1a1aa` | Neoforge `--muted-foreground` |
| `--lg-ink-faint` | `#7f7f89` | |
| `--lg-rule` | `#ffffff1a` | Neoforge `--border`, unchanged |
| `--lg-rule-strong` | `#ffffff2e` | |
| `--lg-brand` | `#e36414` | Neoforge `--primary`, unchanged. 5.75:1 on paper |
| `--lg-brand-hover` | `#f07d2e` | |
| `--lg-brand-soft` | `#2a1408` | |
| `--lg-brand-rule` | `#5c2d0e` | |
| `--lg-refuse` | `#f05a72` | crimson, hue 348 |
| `--lg-agree` | `#5ecf92` | unchanged, hue 150 |
| `--lg-notice` | `#d9bf4e` | yellow, hue 48 |

### Light (kept, retuned)

| Token | Value | Note |
|---|---|---|
| `--lg-paper` | `#f5f5f0` | the brand's own cream, replacing `#fbfaf7` |
| `--lg-paper-sunken` | `#ebebe4` | |
| `--lg-paper-raised` | `#ffffff` | |
| `--lg-ink` | `#0a0a0b` | the brand's near-black |
| `--lg-ink-muted` | `#55555e` | |
| `--lg-ink-faint` | `#696973` | |
| `--lg-rule` | `#e0ddd3` | |
| `--lg-rule-strong` | `#c9c5b8` | |
| `--lg-brand` | `#a8460a` | 5.42:1 on cream — see below |
| `--lg-brand-hover` | `#8a3907` | |
| `--lg-refuse` | `#b02339` | |
| `--lg-agree` | `#2a774f` | retuned |
| `--lg-notice` | `#7d6808` | |

### Three colour decisions, and why

**Light-mode orange is darker than the brand's.** `#e36414` on `#f5f5f0` is
3.15:1 — it fails AA as link text. `#a8460a` reaches 5.42:1. The brand orange
stays exact on dark, where it passes at 5.75:1, and on the mark at every size.
Only light-mode *text* darkens. Stated plainly because it is a visible
divergence from the brand hex.

**Orange owns hue 23, so the other three signals move away from it.** Refuse
goes to a cool crimson at hue 348; notice goes to yellow at hue 48; agree
stays green at hue 150. That is the widest spread available once brand is
fixed.

**Notice is the tightest pair and must be checked on screen.** 25° from brand
is close. Mitigations: notice is much less saturated, and every notice panel
carries the word *noticed* — colour is never the only cue. If it still reads
as orange on a real diagnostic page, notice becomes a neutral-warm rule
treatment with no fill, rather than being pushed further into green.

## Typography

| Face | Role |
|---|---|
| Playfair Display | display only: `h1`, Hero and Band titles, wordmark, Stat values |
| Source Serif 4 | body prose, unchanged |
| Geist Sans | UI chrome: sidebar, nav, labels, table headers |
| Geist Mono | code, and all journal furniture |

**Body prose stays Source Serif. This is my call, not the brand's.** Playfair
is a Didone with hairline thins that disappear at 19px and fatigue the eye
over an hour; it is a display face and behaves like one. Neoforge's own body
copy is short marketing paragraphs set in Geist, which is a different problem
from 52,000 words. The identity is carried by the display face, the palette
and the furniture — those are what a reader registers — and the prose face is
the one place where readability should win. Flagging it because it is the
clearest deliberate divergence in this design.

Playfair Display loads via `next/font/google` alongside Source Serif, as
`--lg-font-display`.

## The journal system

Taking "go all the way": the furniture is what makes Neoforge feel like
Neoforge, and without it this is a recolour.

- **Masthead** on the landing page, mirroring neoforgelabs.tech's
  `VOL. 01 — A RESEARCH HOLDING / NEOFORGE LABS / MMXXV`: a three-part
  monospace rule with volume, subject and roman-numeral year above the Hero
  title.
- **Section numbering** on every `h2`, as `§ 01`, `§ 02`, generated by CSS
  counters in `globals.css`. This is the significant win: 55 MDX files are
  not touched, and new pages get numbered for free.
- **Figure numbering** on `Diagnostic` and `RunReport`, as `FIG. 01`, also by
  CSS counter, rendered into the existing `note` slot.
- **Footer** gains the locality line (`◉ WESTLANDS, NAIROBI`, `MMXXVI`)
  beside the existing licence and copyright.
- Breadcrumbs are already monospace. Sidebar group headings are already
  uppercase and letterspaced. Both stay; adding numbering there too would
  tip from furniture into costume.

## Components

| File | Change |
|---|---|
| `components/Mark.tsx` | new ligature mark; wordmark to Playfair |
| `components/Hero.*` | Playfair title; masthead; retuned actions |
| `components/Diagnostic.*` | plate aesthetic; `FIG.` numbering; retuned refuse |
| `components/RunReport.*` | plate aesthetic; `FIG.` numbering |
| `components/Stats.*` | Playfair numerals |
| `components/Note.*`, `Hazard.*`, `Lede.*` | repaint via tokens only |
| `app/tokens.css` | the palette above; `--lg-font-display` |
| `app/globals.css` | dark-first base; `§` and `FIG.` counters; Playfair on `h1` |
| `app/layout.tsx` | `<Head color>` to hue 23; `backgroundColor`; Playfair; `defaultTheme: 'dark'`; OG image metadata |
| `next.config.mjs` | Shiki themes retuned to the new surfaces |

## Assets to produce

In `web/public/`, which must be created:

- `favicon.svg` — mark, with an internal `prefers-color-scheme` block so it
  inverts for light tab strips
- `icon.svg`, `apple-touch-icon.png` (180×180, near-black plate)
- `og.png` (1200×630) — near-black plate, mark, `Ligase` in Playfair, orange
  hairline, one line of description

`layout.tsx` already declares `twitter: { card: 'summary_large_image' }` with
no image behind it. That is a live bug; this change fixes it.

## Documentation

Required by CLAUDE.md, in the same change:

- `web/content/project/architecture/identity.mdx` — the mark's construction
  and variants, the palette and what each signal means, the journal system,
  and the three colour decisions and the prose-face decision with their
  reasons. Added to `project/architecture/_meta.ts`.
- `CHANGELOG.md` under `[Unreleased] / ### Changed`.

**No `concepts/` or `guide/` page.** CLAUDE.md asks for all three per
feature, but a visual identity has no API to explain and no task to walk
through; those pages would be padding, and `project/` is explicitly the
section "written about Ligase, not for someone using it." My call, flagged
here rather than made silently.

## Verification

- `npm run check` in `web/` — the real gate. Regenerates, builds,
  link-checks, fails on orphan pages and dangling anchors.
- Every foreground/background token pair measured against WCAG AA, both
  modes. The ratios above are computed, not estimated, and are to be
  re-measured against what actually ships.
- All five generators still produce output.
- A `Diagnostic`, a `RunReport` and a `Stats` row looked at in both modes,
  on a real page, before the change is called done. The notice-versus-brand
  pair is checked here specifically.

## Left to the builder

- Exact tints for `brand-soft` / `refuse-soft` / `agree-soft` / `notice-soft`
  and their rules in both modes; the table above fixes the foregrounds, the
  tints follow from them.
- The specific Shiki theme pair, or a tuned theme, against cream and
  near-black.
- Masthead wording. `VOL. 02 — THE ENGINE` is a placeholder; the volume
  number in particular is a brand decision the author should make.
- Whether the sidebar's active-item rule reads better in brand orange or in
  cream against a near-black sidebar.
- Where exactly `FIG.` numbering should reset — per page is the assumption.

## Builder check — calls made in review

A reviewer planned from this document and found ten gaps. Six are settled
here, as my calls rather than the author's.

**`FIG.` numbering gets its own slot; the MDX is not touched.** `Diagnostic`'s
`note` prop is load-bearing on all seven call sites (`note="ligase check —
nothing ran"`), so the figure number cannot replace it. `Diagnostic` gains a
second slot in its existing bar; `RunReport` gains a header bar it currently
lacks. Both are additive component changes, which keeps the "prose untouched"
non-goal intact.

**`--lg-ink-faint` is corrected to meet AA.** As first specified it was
`3.92:1` dark and `3.34:1` light — both failures, and it is exactly where the
journal furniture lives (TOC, breadcrumbs, sidebar group headings, the mark's
letterforms). AA is binding, so: dark `#8b8b95`, light `#6b6b75`. The
furniture is slightly louder than intended; that is the correct trade.

**`Note kind="note"` comes off brand.** With brand as the link colour, the
`ran` colour and 25° from `notice`, five semantics were competing for four
signals. The default callout moves to a neutral rule-and-ink treatment with no
tint.

**The refusal device is the 404 page only.** "Build failures" had no surface
in this site; the generators fail as text in CI. `scripts/check-links.mjs`
exempts `/404` from the orphan rule, so this is safe.

**`VOL. 02 — THE ENGINE` ships as a knowing placeholder**, flagged in
`CHANGELOG.md`, rather than inventing a volume number that contradicts the
parent site.

**Both notice treatments get built** — the yellow fill and the neutral-warm
rule — so the on-screen judgement has something to choose between. The author
decides; the plan carries a review gate before the change is called done.

### Still open — author's call

1. **A source of truth for neoforgelabs.tech.** The repo contains only URL
   strings. Colours and typefaces are in this document, but the furniture is
   metrics: eyebrow letterspacing, hairline weights, masthead proportions,
   Playfair sizes per level, card padding. Without a tokens file, the site's
   repo, or full-page screenshots, "matches exactly" is unverifiable and the
   standard becomes "plausibly the same family."
2. **How `og.png` and `apple-touch-icon.png` get produced.** There is no
   raster toolchain: static export, no `sharp`/`satori`/`resvg`, and CI runs
   only `npm ci` plus the Python generators. Commit binaries (nothing can
   regenerate them, against this site's posture), add a renderer (a
   dependency, which CLAUDE.md says to ask about), or ship SVG-only and leave
   the Twitter-card bug unfixed.
3. **Whether the sub-brand card lockup ships here.** It is specified under
   Lockups but appears in no deliverable list, and the grid it drops into is
   on another site. A documented spec, a built component, and an exported
   asset for elsewhere are three different jobs.
4. **One geometry at every size, or a tuned small variant.** At 16px the tie
   renders ~1.4px and the serif slabs ~0.55px. The 16px favicon is this
   design's hardest goal and the tie is what is meant to carry it, so an
   optically-corrected variant under 32px may be needed — a second drawing,
   and the mark is the author's.

### Setup note

The gate is not runnable yet: `web/node_modules` is absent, `pagefind` is not
installed, and `import ligase` fails. `npm run check` needs `npm ci`,
`pip install -e ".[docs]"` and a Rust build of the extension first. The
both-modes visual check is therefore not a five-minute confirmation at the
end; it needs that setup in front of it.

## Brand reference — verified against source

Source of truth: `github.com/cod3smith/neoforgelabs`, `frontend/`. A Vite +
Tailwind + shadcn app. The values below are read from it, and supersede any
earlier guess in this document.

### Corrections to earlier guesses

- **The eyebrow is orange, not grey.** `.eyebrow` in `frontend/src/index.css`
  is `10px`, uppercase, `letter-spacing: 0.28em`, `color: #E36414`. The grey
  eyebrows seen on screen are a separate treatment: `font-mono text-[10px]
  uppercase tracking-[0.22em] text-[#A1A1AA]`, used for nav, card codes and
  the locality line.
- **Display is set light with tight tracking** (`font-light tracking-tight`,
  `tracking-tighter` at hero size). The brand's CSS asks for weight 300; see
  the display-weight amendment at the end of this document for what that
  actually resolves to, and what the docs site does instead.
- **The card hairline is 48px, not 52px** (`h-px w-12`), and animates to 96px
  (`w-24`) on hover over 500ms.
- **There are two texture layers I had missed entirely**: `.grain`, a
  `feTurbulence fractalNoise` overlay at `opacity: 0.035`, fixed, 256px tile;
  and `.aurora`, three orange radial gradients at 0.04–0.08 alpha under
  `blur(60px)`.

### The core typographic device

`Wordmark.tsx` sets `NEOFORGE LABS` with characters at index 3 and 9 — the
`F` of Forge and the `L` of Labs — in **italic**, turning `#E36414` on hover.
`Nav.tsx` does the same more plainly: `Neoforge <span class="italic
text-[#E36414]">Labs</span>`. `SubBrands.tsx` carries it into headings:
`Three labs. <span class="italic text-[#E36414]">One</span> thesis.`

**Italic plus orange, on one meaningful letter or word, is the brand's
signature move.** This is the single most important finding, and it changes
the mark: the device is not merely a coloured element, it is a *slanted* one.

### Measured values

| Thing | Value |
|---|---|
| Eyebrow (section) | 10px, uppercase, `0.28em`, `#E36414` |
| Mono UI (nav, codes, locality) | 11px / 10px, uppercase, `0.22em`, `#A1A1AA` |
| Nav wordmark | `font-display`, 18→20px, `tracking-tight` |
| Nav bar | `h-16`; on scroll `backdrop-blur-xl`, `bg-[#0A0A0B]/70`, `border-b border-white/5` |
| Section h2 | `font-display`, 36→60px, `font-light`, `tracking-tight`, `mt-4` after eyebrow |
| Card | `aspect-[3/4]`, `p-6 md:p-8`, `border-top: 2px solid <brand colour>` |
| Card image | `object-cover opacity-70` → `0.9` on hover, 500ms |
| Card scrim | `linear-gradient(to top, #0A0A0B, #0A0A0B66, transparent)` |
| Card title | `font-display`, 30→36px, `font-light`, `tracking-tight` |
| Card hairline | 1px × 48px `#E36414` → 96px on hover, 500ms |
| CTA button | `px-4 py-2`, `border-white/15`, 11px mono `0.22em`; hover border+text `#E36414` |
| Radius | `0.625rem` (10px) |
| Section rhythm | `py-24 md:py-32`, heading block `mb-12 md:mb-20` |

### Sub-brand codes and card colours

Deep and desaturated, except the flagship which takes the brand orange:

| Brand | Code | Card colour |
|---|---|---|
| NeoFarm | `01 / AGRI-TECH` | `#2D4233` |
| NeoHealth | `02 / HEALTH-TECH` | `#8A3D20` |
| NeoForge | `03 / FINTECH & LOGISTICS` | `#E36414` |
| NeoRx | `04 / DRUG DISCOVERY` | `#1E3555` |
| NeoRoute | `05 / LOGISTICS` | `#1A3040` |
| **Ligase** | **`06 / COMP BIO`** | **`#332C5C`** |

Ligase takes a deep violet. It is unclaimed in the family, it sits in the same
muted register as the navies and the green, and it lets the docs site's
original violet survive as Ligase's card colour while orange remains the brand
accent. Continuity without a second accent to justify.

## The four open items, resolved

### 1. Source of truth — found

See above. The acceptance standard is now "matches", not "plausibly the same
family". The furniture is a transcription task against measured values.

### 2. Raster assets — generated, not committed

`og.png` and `apple-touch-icon.png` are produced by Next's built-in
`ImageResponse` from `next/og`, via `app/opengraph-image.png/route.tsx` and
`app/apple-icon.png/route.tsx`.

This is the best option and it **adds no dependency**: `next/og` ships inside
Next 15, which is already here, and it renders at build time under
`output: 'export'`. No `sharp`, no `satori`, no `resvg`, no CI change, and
nothing asked of CLAUDE.md's dependency rule.

It also fits this site's posture exactly. Five things here already regenerate
rather than being stored; these become the sixth and seventh, instead of two
binaries nobody can rebuild. `favicon.svg` and `icon.svg` stay hand-authored
SVG, since they must not depend on a renderer.

The OG card: near-black plate, grain overlay, the mark, `Ligase` in Playfair
Light with the italic-orange device, a 48px orange hairline, and one line of
description. 1200×630.

### 3. The sub-brand card lockup — ships here

Built as a real component, `components/BrandCard.tsx`, to the geometry in the
table above, and rendered on `identity.mdx` so the lockup is visible rather
than only described. `scripts/check-links.mjs` fails on a page nothing reaches,
so `identity.mdx` gets its `project/architecture/_meta.ts` entry in the same
change — which it needs anyway.

### 4. The mark — two optical sizes

A tuned small variant, because the 16px favicon is this design's hardest goal
and one geometry cannot serve both ends.

- **Display variant** (≥32px): the geometry already specified, with the tie
  **slanted to match the brand's italic device** rather than horizontal. This
  is the change the source reading forces, and it makes the mark unmistakably
  Neoforge.
- **Small variant** (<32px): tighter viewBox, serif slabs thickened from 3.5
  to 6 units, tie from 9 to 14, tittle enlarged, slant reduced so it survives
  rasterisation. At 16px the display variant renders its tie at ~1.4px and its
  slabs at ~0.55px, which is why it cannot be reused.

Both ship as separate files; `favicon.svg` carries the small variant,
everything else the display one. `identity.mdx` documents when each applies.

## Amendment, 2026-10-21 — four values failed this document's own gate

Implementing Task 2 ran the palette through `scripts/check-contrast.mjs`, the
gate this document's Verification section demands, and four of the values in
the tables above failed it. They have been corrected in place; the originals
are recorded here because the reasoning is worth keeping.

| Token | Was | Now | Why |
|---|---|---|---|
| dark `ink-faint` | `#6e6e78` | `#7f7f89` | 3.54:1 on `paper-raised`; now 4.51:1 |
| light `ink-faint` | `#85858f` | `#696973` | 3.05:1 on `paper-sunken` |
| light `agree` | `#2b7a51` | `#2a774f` | 4.38:1 on `paper-sunken` |
| light `notice` | `#7d6508` | `#7d6808` | 24.9° from brand; the gate requires 25°, now 26.4° |

The AA requirement outranks the specific hexes: the ratios were the
commitment and the values were a means to them. Three of the four move by a
step invisible to the eye. The fourth, dark `ink-faint`, lands at 4.51:1 with
almost no headroom — any later change to `paper-raised` will break it, and the
gate is what will say so.

This is the second time a measured value in this document proved wrong under
test; the first was two self-test expectations in the plan's Task 1. Both were
caught because the gate was built before the palette it judges. That ordering
was the right call and should be kept for any future palette work.

## Amendment, 2026-10-21 — the display weight, and what Neoforge actually renders

Reviewing Task 5 surfaced that this document specified a weight that does not
exist. `https://fonts.googleapis.com/css2?family=Playfair+Display:wght@300`
returns **HTTP 400**: the Playfair Display family is 400–900. Weight 300 lives
only in the newer, differently-drawn "Playfair" family. `next/font/google`
would have failed the build.

The display face is therefore **Playfair Display 400**, its lightest real
weight, with italic. Switching to the "Playfair" family instead would have
changed the typeface rather than the weight.

Behind that sits a finding about the parent brand worth recording. Neoforge
Labs does not load Playfair from Google either. `frontend/src/index.css`
declares:

    @font-face {
      font-family: 'Display';
      src: local('Playfair Display'), local('Georgia'), local('Times New Roman');
      font-weight: 300 900;
    }

Every source is `local()`, and the 300–900 range is asserted, not supplied.
Playfair Display is rarely installed locally, so for most visitors
neoforgelabs.tech renders its display type in **Georgia**, and its
`font-light` is nominal. This document's earlier claim that the brand is
"Playfair Display Light 300" described the CSS, not the pixels.

The docs site serves real webfonts and should not copy that arrangement: a
display face that silently differs per machine is not an identity. It loads
Playfair Display 400 properly and gets the same letterforms everywhere, which
is closer to the brand's intent than the brand's own implementation is.
