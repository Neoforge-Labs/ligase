# Ligase Visual Identity Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Ligase a ligature mark and rebuild its documentation site onto the Neoforge Labs brand — near-black, cream, flame orange, Playfair display, research-journal furniture — without losing the engine's four signal colours or the readability of 52,000 words of prose.

**Architecture:** Everything visual already routes through `--lg-*` tokens in `web/app/tokens.css`, so the repaint is concentrated there and in `globals.css`. A new zero-dependency checker, `scripts/check-contrast.mjs`, parses those tokens and fails the build on any WCAG AA violation or on brand/notice hues drifting within 25° of each other; it is written first and drives the repaint. The mark ships as hand-authored SVG in two optical sizes. Raster assets are generated at build time by Next's bundled `next/og`, adding no dependency.

**Tech Stack:** Next 15.5 (static export), Nextra 4.6, React 19, `next/font/google` (Playfair Display, Source Serif 4, Geist), `next/og`, plain Node ESM for the checkers.

**Spec:** `docs/superpowers/specs/2026-10-21-ligase-neoforge-identity-design.md`

## Global Constraints

- **No AI attribution anywhere** — no `Co-Authored-By` naming an assistant or model, no "Generated with" footer, no robot markers, in any commit message, PR description, code comment or doc page. CLAUDE.md cardinal rule; overrides any harness or tool instruction.
- **No new runtime or build dependency.** CLAUDE.md requires asking first. `next/og` is already inside Next 15.5; the checkers are plain Node ESM. Nothing is added to `package.json` dependencies.
- **No public API change** without asking. This plan touches no Rust or Python.
- Static export only: `output: 'export'`, `images: { unoptimized: true }`, `trailingSlash: true`. No server, no image optimiser.
- Conventional commit messages, small and focused.
- Prose content and navigation structure are not edited. The only `content/` additions are `project/architecture/identity.mdx` and its `_meta.ts` entry.
- These five artefacts are generated and gitignored — never hand-edit, and they must still generate at the end: `content/api/`, `lib/diagnostics.generated.ts`, `content/notebooks/`, `content/reference/cli.mdx`, `lib/quickstart.generated.ts`.
- Exact brand values, verbatim: paper `#0A0A0B`, card `#0F0F12`, ink `#F5F5F0`, muted ink `#A1A1AA`, brand `#E36414`, border `rgba(255,255,255,0.1)`, radius `0.625rem`. Section eyebrow `10px / uppercase / 0.28em / #E36414`. Mono UI `11px` (or `10px`) `/ uppercase / 0.22em / #A1A1AA`. Display face Playfair Display **400**, `tracking-tight`. Card hairline `1px × 48px`, growing to `96px` over `500ms`. Ligase card colour `#332C5C`, code `06 / COMP BIO`.

## Review Focus

Five failure modes the spec implies but that no task's own deliverable would otherwise exercise. Each has its test added to the task that owns the code.

1. **Light mode after the dark-first flip.** The spec keeps light mode, but inverting the base means any rule still assuming warm paper is now an orphan. A reader with `prefers-color-scheme: light`, or who hits the theme toggle, expects a complete cream theme, not a half-converted one. → Task 2.
2. **`prefers-reduced-motion`.** The brand adds a grain overlay, an aurora gradient and 500ms hover growth. `globals.css` already honours reduced motion; a reader who sets it expects the new motion to stop too. → Task 4.
3. **Printing.** 52,000 words on a near-black page prints as a black rectangle. No print stylesheet exists today. Someone printing a guide expects ink on paper. → Task 4.
4. **The `§` counter on pages with no `h2`, or whose first heading is `h3`.** Generated pages (`content/api/`, `reference/cli.mdx`) are shaped by generators, not authors. A counter that leaks across pages or renders `§ 00` is visible nonsense on a page nobody hand-wrote. → Task 4.
5. **The favicon on a light browser tab strip.** The small variant is cream letterforms; on a light tab strip it disappears entirely. The in-SVG `prefers-color-scheme` block is the fix and is easy to get subtly wrong. → Task 3.

---

## File Structure

| File | Responsibility |
|---|---|
| `web/scripts/check-contrast.mjs` | **Create.** Parses `tokens.css`, computes WCAG ratios and brand/notice hue separation, exits non-zero on violation |
| `web/app/tokens.css` | **Modify.** Both palettes; `--lg-font-display` |
| `web/app/globals.css` | **Modify.** Dark-first base, `§`/`FIG.` counters, eyebrow and mono classes, grain, aurora, print |
| `web/app/layout.tsx` | **Modify.** Playfair, `<Head color>`, `backgroundColor`, theme-color, dark default, OG metadata |
| `web/app/opengraph-image.png/route.tsx` | **Create.** 1200×630 via `next/og` |
| `web/app/apple-icon.png/route.tsx` | **Create.** 180×180 via `next/og` |
| `web/app/not-found.tsx` | **Create.** 404 carrying the refusal device |
| `web/public/favicon.svg` | **Create.** Small variant, both tab-strip schemes |
| `web/public/icon.svg` | **Create.** Display variant |
| `web/components/Mark.tsx` | **Modify.** Ligature mark + Playfair wordmark |
| `web/components/BrandCard.tsx` | **Create.** The sub-brand card lockup |
| `web/components/{Diagnostic,RunReport,Stats,Note,Hero}.*` | **Modify.** `FIG.` slots, plate aesthetic, Playfair, `note` off brand |
| `web/content/project/architecture/identity.mdx` | **Create.** The identity, documented |
| `web/app/sitemap.ts`, `web/app/robots.ts` | **Create.** Static sitemap and an explicit AI-crawler allowlist |
| `web/scripts/generate-llms.mjs` | **Create.** `llms.txt`, `llms-full.txt`, per-page markdown |
| `web/scripts/check-seo.mjs` | **Create.** Gates discovery, machine-readable prose and the honesty rule |
| `CHANGELOG.md` | **Modify.** `[Unreleased] / ### Changed` |

---

### Task 1: The contrast gate

The repaint's test. Written first so Task 2 has something to fail against.

**Files:**
- Create: `web/scripts/check-contrast.mjs`
- Modify: `web/package.json` (scripts only — no dependencies)

**Interfaces:**
- Consumes: nothing.
- Produces: `npm run contrast` in `web/`, exiting `0` when every pair passes and `1` with a per-failure report otherwise. Wired into `npm run check`. Task 2 is complete only when this passes.

- [ ] **Step 1: Write the failing test**

Create `web/scripts/check-contrast.mjs` with a `--self-test` flag that runs assertions against known values, so the checker's own maths is verified before it judges the palette:

```js
const CASES = [
  { fg: '#E36414', bg: '#F5F5F0', expect: 3.15, label: 'brand orange on cream fails AA' },
  { fg: '#E36414', bg: '#0A0A0B', expect: 5.75, label: 'brand orange on near-black passes AA' },
  { fg: '#F5F5F0', bg: '#0A0A0B', expect: 18.9, label: 'cream on near-black' },
  { fg: '#ffffff1a', bg: '#0A0A0B', expect: 1.19, label: '8-digit hex composites over its surface' },
]
// each: assert Math.abs(contrast(fg, bg) - expect) < 0.05
// plus: assert hueOf('#E36414') is within 1 of 23
// plus: assert separation('#E36414', '#d9bf4e') >= 25
```

- [ ] **Step 2: Run it to make sure it fails**

Run: `cd web && node scripts/check-contrast.mjs --self-test`
Expected: FAIL — the file has no `contrast`, `hueOf` or `separation` yet.

- [ ] **Step 3: Implement the checker**

In `web/scripts/check-contrast.mjs`, plain Node ESM, no imports beyond `node:fs`:

- `parseTokens(css: string) => { light: Record<string,string>, dark: Record<string,string> }` — reads `:root` and `.dark` blocks for `--lg-*` declarations.
- `srgbToLinear(c: number) => number`, `relativeLuminance(hex: string) => number`, `contrast(fg: string, bg: string) => number` — WCAG 2.1. An 8-digit hex or `rgba()` foreground is composited over `bg` before measuring; that is why `#ffffff1a` on `#0A0A0B` is 1.19 and not a division by zero.
- `hueOf(hex: string) => number` — HSL hue in degrees.
- `separation(a: string, b: string) => number` — smallest angular distance between two hues, so 350° and 10° are 20 apart.
- `check(tokens, mode) => Violation[]` — asserts, per mode:
  - `ink`, `ink-muted`, `ink-faint`, `brand`, `refuse`, `agree`, `notice` each ≥ **4.5:1** against `paper`, `paper-raised` and `paper-sunken`. These are all used as normal-size text.
  - each signal ≥ 4.5:1 against its own `-soft` tint.
  - `separation(brand, notice)` ≥ **25** and `separation(brand, refuse)` ≥ **25**. The spec flags notice-versus-brand as the tightest pair; this is what stops it regressing.
  - `rule` and `rule-strong` are exempt — they are hairlines, not text.

Report every violation with mode, pair, measured ratio and required ratio, then `process.exit(1)`.

- [ ] **Step 4: Run the self-test to verify it passes**

Run: `cd web && node scripts/check-contrast.mjs --self-test`
Expected: PASS, all cases.

- [ ] **Step 5: Run it against the current palette**

Run: `cd web && node scripts/check-contrast.mjs`
Expected: FAIL. The current violet palette is not the brand palette; record the violations in the commit body. This is the red test Task 2 turns green.

- [ ] **Step 6: Wire it into the gate**

In `web/package.json`, add `"contrast": "node scripts/check-contrast.mjs"` and change `"check"` to `"npm run build && npm run links && npm run contrast"`.

- [ ] **Step 7: Commit**

```bash
git add web/scripts/check-contrast.mjs web/package.json
git commit -m "test: gate the palette on WCAG AA and hue separation"
```

---

### Task 2: The palette

**Files:**
- Modify: `web/app/tokens.css`
- Modify: `web/app/layout.tsx:80-90` (the `<Head color>` block and theme-color metas)

**Interfaces:**
- Consumes: `npm run contrast` from Task 1.
- Produces: the full `--lg-*` token set in both modes, and `--lg-font-display`. Every later task reads these; none hardcodes a brand hex except the SVG mark files, which cannot use CSS variables.

- [ ] **Step 1: Write the failing test**

Extend `check-contrast.mjs`'s expectations with the values the spec fixes, so the checker asserts the brand is actually present and not merely self-consistent:

```js
const REQUIRED = {
  dark:  { paper: '#0a0a0b', ink: '#f5f5f0', 'ink-muted': '#a1a1aa', brand: '#e36414' },
  light: { paper: '#f5f5f0', ink: '#0a0a0b' },
}
// assert each token equals the brand value exactly, case-insensitive
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && npm run contrast`
Expected: FAIL on both the required-value assertions and the AA violations from Task 1 Step 5.

- [ ] **Step 3: Repaint `tokens.css`**

Replace the `:root` and `.dark` signal and neutral blocks with the two tables in the spec's "The palette" section, verbatim, keeping the existing comment structure and updating its prose to describe the new reasoning. `:root` is now the **light** theme and `.dark` the default; the four signal roles keep their names and meanings. Add `--lg-font-display: var(--font-playfair), 'Playfair Display', Georgia, serif;`.

Derive each `-soft` and `-rule` tint from its signal so the checker's tint assertions pass; the spec leaves the exact tints to the builder.

- [ ] **Step 4: Flip the default and re-point Nextra's accent**

In `web/app/layout.tsx`: set `<Head color>` to `hue: { light: 23, dark: 23 }`, `saturation: { light: 89, dark: 84 }`, `lightness: { light: 35, dark: 48 }`; `backgroundColor` to `{ light: '#f5f5f0', dark: '#0a0a0b' }`; update both `theme-color` metas to match; pass `nextThemes={{ defaultTheme: 'dark' }}` to `<Layout>`. The existing comment explaining why these live here rather than in CSS stays true — update its values, not its argument.

- [ ] **Step 5: Run the gate to verify it passes**

Run: `cd web && npm run contrast`
Expected: PASS, both modes, including hue separation.

- [ ] **Step 6: Test light mode is complete (Review Focus 1)**

Add `scripts/check-contrast.mjs --orphans`, which fails if any `--lg-*` token declared in `.dark` has no counterpart in `:root` or vice versa. A half-converted theme is exactly the failure mode, and a missing token is how it shows up.

Run: `cd web && node scripts/check-contrast.mjs --orphans`
Expected: PASS — both modes declare the same token set.

- [ ] **Step 7: Commit**

```bash
git add web/app/tokens.css web/app/layout.tsx web/scripts/check-contrast.mjs
git commit -m "feat: repaint the docs palette onto the Neoforge brand"
```

---

### Task 3: The mark

**Files:**
- Modify: `web/components/Mark.tsx`, `web/components/Mark.module.css`
- Create: `web/public/favicon.svg`, `web/public/icon.svg`
- Modify: `web/app/layout.tsx` (Playfair via `next/font/google`; icon metadata)

**Interfaces:**
- Consumes: `--lg-font-display` and the palette from Task 2.
- Produces: `<Mark />`, unchanged in signature (no props), rendering the display variant plus the wordmark. Task 6 reuses it; Task 7 reproduces its geometry inline, since `next/og` cannot import a component that relies on CSS modules.

- [ ] **Step 1: Write the failing test**

Create `web/scripts/check-mark.mjs`, asserting the geometry the spec fixes and the thing most likely to be got wrong:

```js
// favicon.svg must carry BOTH tab-strip schemes (Review Focus 5)
assert(/prefers-color-scheme:\s*dark/.test(favicon))
assert(/#0A0A0B/i.test(favicon), 'light tab strip needs dark letterforms')
assert(/#F5F5F0/i.test(favicon), 'dark tab strip needs cream letterforms')
// the tie is slanted, not flat, in both files
assert(/<polygon/.test(favicon) && /<polygon/.test(icon))
// small variant is the thicker one: its slabs are 6 units, the display variant's 3.5
assert(/height="6"/.test(favicon))
assert(/height="3\.5"/.test(icon))
// neither file depends on a webfont
assert(!/font-family|<text/.test(favicon + icon))
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-mark.mjs`
Expected: FAIL — neither SVG exists.

- [ ] **Step 3: Author the two SVG files**

`web/public/icon.svg`, the display variant, 100×100 viewBox: tall stem `x30 y14 w10 h68`; head serif `x24 y14 w22 h3.5`; foot serif `x22 y78.5 w26 h3.5`; short stem `x62 y42 w10 h40`; head serif `x56 y42 w22 h3.5`; foot serif `x54 y78.5 w26 h3.5`; tittle `cx67 cy28 r6`; tie `<polygon points="30,56 72,48 72,57 30,65">` in `#E36414`. Letterforms `#F5F5F0`.

`web/public/favicon.svg`, the small variant, 100×100 viewBox: tall stem `x26 y10 w16 h76`; head serif `x18 y10 w32 h6`; foot serif `x15 y80 w38 h6`; short stem `x60 y44 w16 h42`; head serif `x52 y44 w32 h6`; foot serif `x49 y80 w38 h6`; tittle `cx68 cy26 r9`; tie `<polygon points="26,58 76,52 76,66 26,72">`. Letterforms default to `#0A0A0B` with an internal `<style>@media (prefers-color-scheme: dark){...#F5F5F0}</style>`, so the mark is visible on light and dark tab strips alike. The tie stays `#E36414` in both.

- [ ] **Step 4: Run the test to verify it passes**

Run: `cd web && node scripts/check-mark.mjs`
Expected: PASS.

- [ ] **Step 5: Rebuild `Mark.tsx`**

Replace the four-rect glyph with the display geometry above, inline, using `currentColor` for letterforms and `var(--lg-brand)` for the tie — so the single-colour variant falls out of the same component. The wordmark becomes `Lig` + `<em>ase</em>`: Playfair Display 400, `letter-spacing: -0.02em`, with the `ase` span italic and in `var(--lg-brand)`, matching how `Nav.tsx` sets `Neoforge <span class="italic">Labs</span>`. Replace the component's docstring: the old one argues for two offset bars over a double helix, which is no longer what this draws.

- [ ] **Step 6: Load Playfair and declare the icons**

In `layout.tsx`, add `Playfair_Display` from `next/font/google` with `weight: ['400']`, `style: ['normal','italic']`, `variable: '--font-playfair'`, `display: 'swap'`, and add its `.variable` to the `<html>` className alongside the existing three. Point `metadata.icons` at `/favicon.svg` and `/icon.svg`.

- [ ] **Step 7: Verify it compiles and renders**

Run: `cd web && npm run typecheck && node scripts/check-mark.mjs`
Expected: PASS both.

- [ ] **Step 8: Commit**

```bash
git add web/components/Mark.tsx web/components/Mark.module.css web/public/ web/app/layout.tsx web/scripts/check-mark.mjs
git commit -m "feat: draw the Ligase ligature mark in two optical sizes"
```

---

### Task 4: The journal furniture

**Files:**
- Modify: `web/app/globals.css`

**Interfaces:**
- Consumes: the palette and `--lg-font-display` from Task 2.
- Produces: the classes `.lg-eyebrow` (section, orange) and `.lg-mono` (UI, muted), the `lg-section` and `lg-figure` CSS counters, and `.lg-grain` / `.lg-aurora`. Tasks 5 and 6 use these rather than redeclaring type.

- [ ] **Step 1: Write the failing test**

Extend `check-mark.mjs` — or add `scripts/check-furniture.mjs` — asserting the measured values and the three Review Focus conditions:

```js
// measured from the brand source, exactly
assert(/\.lg-eyebrow[^}]*letter-spacing:\s*0\.28em/.test(css))
assert(/\.lg-eyebrow[^}]*font-size:\s*10px/.test(css))
assert(/\.lg-mono[^}]*letter-spacing:\s*0\.22em/.test(css))
// Review Focus 2: new motion respects the existing reduced-motion block
assert(/prefers-reduced-motion[^}]*\}[\s\S]*lg-aurora|lg-aurora[\s\S]*prefers-reduced-motion/.test(css))
assert(/@media \(prefers-reduced-motion: reduce\)[\s\S]*\.lg-grain::before[\s\S]*display:\s*none/.test(css))
// Review Focus 3: a print stylesheet exists and forces ink on paper
assert(/@media print[\s\S]*background:\s*#fff[\s\S]*color:\s*#000/.test(css))
// Review Focus 4: the section counter resets per page and hides at zero
assert(/counter-reset:\s*lg-section/.test(css))
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-furniture.mjs`
Expected: FAIL — none of these rules exist.

- [ ] **Step 3: Flip the base and add the furniture**

In `globals.css`:
- `html`/`body` keep reading `--lg-paper` and `--lg-ink`, so the dark-first flip needs no change here; update the file's header comment, which currently claims warm paper.
- `.lg-eyebrow`: `10px`, uppercase, `letter-spacing: 0.28em`, `color: var(--lg-brand)`, `font-family: var(--lg-font-mono)`.
- `.lg-mono`: `11px`, uppercase, `letter-spacing: 0.22em`, `color: var(--lg-ink-muted)`.
- `h1` and the Hero/Band titles take `var(--lg-font-display)` at weight 400 with `letter-spacing: -0.02em`. Body prose rules are **not** touched — Source Serif stays, per the spec's stated divergence.
- Section numbering: `.nextra-content { counter-reset: lg-section lg-figure }` and `.nextra-content h2::before { counter-increment: lg-section; content: '§ ' counter(lg-section, decimal-leading-zero) ' — '; }`, styled as `.lg-eyebrow`, displayed above the heading.
- Figure numbering, for Task 5 to consume: `.lg-figure::before { counter-increment: lg-figure; content: 'FIG. ' counter(lg-figure, decimal-leading-zero); }`, styled as `.lg-mono`. Declared here with the other counter so both reset on the same element and cannot drift apart.
- `.lg-grain::before` and `.lg-aurora` exactly as the brand source defines them: `feTurbulence fractalNoise`, `baseFrequency 0.85`, `numOctaves 4`, `opacity 0.035`, 256px tile; three orange radials at 0.08/0.05/0.04 under `blur(60px)`.

- [ ] **Step 4: Handle the three Review Focus conditions**

- **Reduced motion:** extend the existing `@media (prefers-reduced-motion: reduce)` block to set `.lg-grain::before { display: none }` and `.lg-aurora { display: none }`. A fixed noise overlay is motion-adjacent and a known migraine trigger; the spec's own block already establishes the site honours this.
- **Print:** add `@media print` forcing `background: #fff`, `color: #000`, hiding grain, aurora, sidebar and TOC, and removing the `§` counter's colour. 52,000 words that cannot be printed is a regression the dark flip would otherwise introduce silently.
- **The counter on generated pages:** scope the counter to `.nextra-content h2` only and reset it on `.nextra-content`, so it restarts per page and never reaches a page with no `h2`. Verify against a generated page in Step 5.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web && node scripts/check-furniture.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/app/globals.css web/scripts/check-furniture.mjs
git commit -m "feat: set the docs in the Neoforge journal style"
```

---

### Task 5: The components

**Files:**
- Modify: `web/components/Diagnostic.tsx`, `Diagnostic.module.css`
- Modify: `web/components/RunReport.tsx`, `RunReport.module.css`
- Modify: `web/components/Stats.module.css`, `Note.module.css`, `Hero.module.css`

**Interfaces:**
- Consumes: `.lg-eyebrow`, `.lg-mono`, the `lg-figure` counter from Task 4.
- Produces: `Diagnostic` gains `figure?: boolean` (default `true`); `RunReport` gains `note?: ReactNode` and `figure?: boolean`. **`Diagnostic`'s existing `note` prop keeps its meaning and all seven call sites keep working** — the figure number renders in a new slot beside it, never replacing it.

- [ ] **Step 1: Write the failing test**

Add to `scripts/check-furniture.mjs`:

```js
// the seven call sites' note text must survive untouched
const mdx = readAllMdx()
assert(mdx.filter(f => /note="ligase check — nothing ran"/.test(f)).length === 6)
assert(mdx.some(f => /note="nothing ran"/.test(f)))
// FIG. gets its own slot, and RunReport gains a bar it did not have
assert(/lg-figure/.test(diagnosticCss) && /lg-figure/.test(runReportCss))
assert(/counter-increment:\s*lg-figure/.test(globalsCss))
// Review Focus: the default callout is no longer on brand
assert(!/--lg-brand/.test(noteCss.match(/\[data-kind=.note.\][^}]*/)?.[0] ?? ''))
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-furniture.mjs`
Expected: FAIL on the figure-slot and Note assertions; the call-site assertions should already pass and must keep passing.

- [ ] **Step 3: Add the figure slot to `Diagnostic` and a bar to `RunReport`**

`Diagnostic` already renders a bar containing `note`; add a `FIG. NN` element to the left of it, numbered by the `lg-figure` counter, incremented in CSS so no prop threading is needed. `RunReport` currently renders only `.panel > .body` — give it the same bar, taking an optional `note` and the same figure number.

- [ ] **Step 4: Restyle onto the plate aesthetic, and take `note` off brand**

Panels adopt `--lg-paper-raised` with `1px solid var(--lg-rule)` and `border-radius: var(--lg-radius-lg)`; `Stats` values take `var(--lg-font-display)` at 300. In `Note.module.css`, `[data-kind='note']` moves from brand tint and rule to a neutral `--lg-rule-strong` left rule on `--lg-paper-sunken` with `--lg-ink` text — so the default callout stops competing with every link on the page. `warning`, `tip` and `caution` keep their signals.

- [ ] **Step 5: Run the tests and the typecheck to verify they pass**

Run: `cd web && npm run typecheck && node scripts/check-furniture.mjs`
Expected: PASS both. The call-site assertions confirm no MDX was edited.

- [ ] **Step 6: Commit**

```bash
git add web/components/
git commit -m "feat: restyle the engine's panels and number them as figures"
```

---

### Task 6: The card lockup, documented

**Files:**
- Create: `web/components/BrandCard.tsx`, `web/components/BrandCard.module.css`
- Create: `web/content/project/architecture/identity.mdx`
- Modify: `web/content/project/architecture/_meta.ts`, `web/mdx-components.tsx`

**Interfaces:**
- Consumes: `<Mark />` from Task 3; `.lg-mono` from Task 4.
- Produces: `BrandCard({ code, title, tagline, colour, href }: { code: string; title: ReactNode; tagline: ReactNode; colour: string; href?: string })`, registered in the MDX map so `identity.mdx` can render the lockup rather than only describe it.

- [ ] **Step 1: Write the failing test**

```js
// the measured card geometry, exactly
assert(/aspect-ratio:\s*3\s*\/\s*4/.test(cardCss))
assert(/width:\s*48px/.test(cardCss) && /width:\s*96px/.test(cardCss))
assert(/500ms/.test(cardCss))
assert(/border-top:\s*2px/.test(cardCss))
// the page is reachable, or check-links fails the build
assert(/identity/.test(archMeta))
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-furniture.mjs`
Expected: FAIL — the component and the nav entry do not exist.

- [ ] **Step 3: Build `BrandCard`**

To the measured geometry: `aspect-ratio: 3/4`, padding `1.5rem`/`2rem`, `border-top: 2px solid` the `colour` prop, a `.lg-mono` row with `code` left and `→ Open` right, the title in Playfair 400, the tagline at `0.875rem`, and a `1px × 48px` orange hairline growing to `96px` over `500ms` on hover. No image or scrim — the parent site's cards carry photography this component has no equivalent for, and inventing one would be a claim about a product that does not exist.

- [ ] **Step 4: Write `identity.mdx`**

Under `project/`, because it is written about Ligase rather than for someone using it. It documents: the mark's construction and its two optical sizes, with `<BrandCard code="06 / COMP BIO" colour="#332C5C" />` rendered live; the palette table and what each of the four signals means; the journal system; and — stated plainly, with reasons — the four decisions that diverge from the brand or from CLAUDE.md's letter: prose stays Source Serif, light-mode orange darkens to `#a8460a`, `ink-faint` is louder than intended to pass AA, and the default callout came off brand. Add `identity: 'Visual identity'` to `_meta.ts` and `BrandCard` to `mdx-components.tsx`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cd web && npm run typecheck && node scripts/check-furniture.mjs`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add web/components/BrandCard.* web/content/project/architecture/ web/mdx-components.tsx
git commit -m "docs: document the visual identity and ship the card lockup"
```

---

### Task 7: Generated raster assets and the 404

**Files:**
- Create: `web/app/opengraph-image.png/route.tsx`, `web/app/apple-icon.png/route.tsx`, `web/app/not-found.tsx`
- Modify: `web/app/layout.tsx` (`openGraph.images`)

**Interfaces:**
- Consumes: the palette from Task 2; the mark geometry from Task 3, reproduced inline because `next/og` renders without CSS modules or `next/font` variables.
- Produces: `/opengraph-image`, `/apple-icon` and `/404` as static files at export time.

- [ ] **Step 1: Write the failing test**

```js
// no dependency was added (Global Constraint)
const before = JSON.parse(gitShow('HEAD~0:web/package.json'))
assert.deepEqual(Object.keys(pkg.dependencies), Object.keys(before.dependencies))
// after build, the files exist at the right sizes
assert(existsSync('out/opengraph-image.png') || existsSync('out/opengraph-image/index.png'))
assert(pngSize(ogPath).width === 1200 && pngSize(ogPath).height === 630)
assert(pngSize(appleIconPath).width === 180)
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-assets.mjs`
Expected: FAIL — no build output, no routes.

- [ ] **Step 3: Implement the three routes**

`opengraph-image.tsx`: `size = { width: 1200, height: 630 }`, `contentType = 'image/png'`, default export returning `new ImageResponse(...)` from `next/og` — near-black plate, the display mark inline as SVG, `Lig` + italic orange `ase` in a serif, a 48px orange hairline, and one line of description. Playfair is fetched and passed via the `fonts` option, since `next/og` resolves no CSS variables; fall back to the bundled serif if the fetch fails, so a build never breaks on a font CDN.

`apple-icon.tsx`: `size = { width: 180, height: 180 }`, the small variant on a near-black plate.

`not-found.tsx`: the refusal device — the same two letterforms with the tie **broken and offset** instead of bound, in `--lg-refuse` — above a short line and a link home. `scripts/check-links.mjs` exempts `/404` from the orphan rule, so this needs no nav entry.

- [ ] **Step 4: Build and verify**

Run: `cd web && npm run build && node scripts/check-assets.mjs`
Expected: PASS — both rasters present at the right dimensions, `package.json` dependencies unchanged.

- [ ] **Step 5: Commit**

```bash
git add web/app/opengraph-image.tsx web/app/apple-icon.tsx web/app/not-found.tsx web/app/layout.tsx web/scripts/check-assets.mjs
git commit -m "feat: generate the social card, app icon and refusal page"
```

---

### Task 8: Discovery infrastructure

**Files:**
- Create: `web/app/sitemap.ts`, `web/app/robots.ts`
- Create: `web/scripts/check-seo.mjs`
- Modify: `web/app/layout.tsx` (`alternates.canonical`, JSON-LD)

**Interfaces:**
- Consumes: `metadataBase` (already `https://ligase.neoforgelabs.tech`) and the `openGraph.images` set in Task 7.
- Produces: `out/sitemap.xml` and `out/robots.txt` at export time, a canonical URL on every page, and one `SoftwareApplication` JSON-LD block. Task 9 reads the same page list.

Next 15's `app/sitemap.ts` and `app/robots.ts` both emit static files under `output: 'export'`, so this adds no dependency.

- [ ] **Step 1: Write the failing test**

Create `web/scripts/check-seo.mjs`, run against `out/` after a build:

```js
// every content page is in the sitemap, and nothing that 404s is
const urls = parseSitemap(read('out/sitemap.xml'))
assert(urls.length >= 56, 'all 55 mdx pages plus the index')
for (const u of urls) assert(existsSync(outPathFor(u)), `sitemap lists ${u} but no page was exported`)
// the AI crawlers are explicitly admitted, not merely unblocked
for (const bot of ['GPTBot', 'ClaudeBot', 'PerplexityBot', 'Google-Extended', 'CCBot'])
  assert(new RegExp(`User-agent:\\s*${bot}`, 'i').test(robots), `${bot} not named in robots.txt`)
assert(/Sitemap:\s*https:\/\/ligase\.neoforgelabs\.tech\/sitemap\.xml/.test(robots))
// canonicals, and structured data that claims nothing untrue
assert(/<link rel="canonical"/.test(read('out/index.html')))
const ld = JSON.parse(extractJsonLd(read('out/index.html')))
assert(ld['@type'] === 'SoftwareApplication')
assert(!('aggregateRating' in ld) && !('review' in ld), 'no invented ratings — CLAUDE.md')
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-seo.mjs`
Expected: FAIL — no sitemap, no robots, no canonical.

- [ ] **Step 3: Implement the three pieces**

`app/sitemap.ts`: default export `async function sitemap(): Promise<MetadataRoute.Sitemap>`, deriving every route from `getPageMap()` so new pages are listed automatically and the list cannot drift from the nav. Use `trailingSlash` form to match the export.

`app/robots.ts`: default export returning `MetadataRoute.Robots` with `*` allowed, each of the five AI crawlers named and allowed explicitly, and `sitemap` set. Naming them is the point — an unnamed crawler is at the mercy of whatever default it assumes.

`layout.tsx`: add `alternates: { canonical: './' }` to `metadata` so each page emits its own canonical, and one `<script type="application/ld+json">` with a `SoftwareApplication` block — name, description, `applicationCategory`, `operatingSystem`, `programmingLanguage`, the repository URL, and the dual licence. **No `aggregateRating`, no `review`, no `interactionCount`.** The project has no outside users and the structured data must not imply otherwise.

- [ ] **Step 4: Build and verify**

Run: `cd web && npm run build && node scripts/check-seo.mjs`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add web/app/sitemap.ts web/app/robots.ts web/app/layout.tsx web/scripts/check-seo.mjs
git commit -m "feat: make the docs discoverable by crawlers and models"
```

---

### Task 9: Machine-readable prose

**Files:**
- Create: `web/scripts/generate-llms.mjs`
- Modify: `web/package.json` (build script)
- Modify: `web/scripts/check-seo.mjs`

**Interfaces:**
- Consumes: the page list and frontmatter used by Task 8.
- Produces: `out/llms.txt`, `out/llms-full.txt`, and `out/<route>/index.md` for every page. Runs in `npm run build` beside `write-redirects.mjs`, after `next build`.

- [ ] **Step 1: Write the failing test**

Add to `check-seo.mjs`:

```js
// llms.txt follows the convention: H1, blockquote summary, then linked sections
const llms = read('out/llms.txt')
assert(/^# Ligase\n/.test(llms))
assert(/^> /m.test(llms), 'needs a one-paragraph summary blockquote')
assert(/^## Silent failures$/m.test(llms), 'sections mirror the nav')
assert((llms.match(/^- \[.+\]\(https:\/\/.+\): .+$/gm) ?? []).length >= 55)
// the full dump carries real prose, not chrome
const full = read('out/llms-full.txt')
assert(full.length > 150000, '52,000 words of prose should exceed this')
// per-page markdown is clean: no JSX, no imports, no component tags
for (const md of allGeneratedMarkdown()) {
  assert(!/^import /m.test(md), 'MDX imports leaked into the markdown variant')
  assert(!/<(Diagnostic|RunReport|Hero|Band|Hazard|Note|Stat|BrandCard)\b/.test(md),
    'component tags leaked instead of their rendered text')
}
// the honesty rule, automated (CLAUDE.md)
for (const w of ['trusted by', 'used by thousands', 'industry-leading', 'customers love'])
  assert(!new RegExp(w, 'i').test(llms + full), `adoption claim "${w}" is not true of this project`)
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cd web && node scripts/check-seo.mjs`
Expected: FAIL — none of the three artefacts exist.

- [ ] **Step 3: Implement the generator**

`web/scripts/generate-llms.mjs`, plain Node ESM:

- `collectPages() => { route, title, description, body }[]` — walks `content/**/*.mdx`, parses frontmatter, strips the import block.
- `toPlainMarkdown(body: string) => string` — removes MDX component tags, keeping their text children, so a `<Diagnostic>` becomes its diagnostic text and a `<Hazard>` becomes its prose. This is the step that decides whether a model reads your pages or reads your JSX; the test above is what holds it honest.
- `writeIndex(pages)` — `out/llms.txt`: `# Ligase`, a blockquote summary taken from the root `description`, then one `##` section per nav group with `- [Title](absolute url): description` per page, in nav order.
- `writeFull(pages)` — `out/llms-full.txt`: every page's title, URL and plain markdown, concatenated in nav order.
- `writePerPage(pages)` — `out/<route>/index.md`, so a model that follows a link can fetch prose instead of parsing HTML.

- [ ] **Step 4: Wire it into the build**

In `web/package.json`, extend `"build"` to run `node scripts/generate-llms.mjs` after `next build`, alongside `write-redirects.mjs`.

- [ ] **Step 5: Build and verify**

Run: `cd web && npm run build && node scripts/check-seo.mjs`
Expected: PASS, including the adoption-claim assertions.

- [ ] **Step 6: Commit**

```bash
git add web/scripts/generate-llms.mjs web/scripts/check-seo.mjs web/package.json
git commit -m "feat: publish the docs as machine-readable prose"
```

---

### Task 10: Code themes, changelog, and the full gate

**Files:**
- Modify: `web/next.config.mjs`, `CHANGELOG.md`

**Interfaces:**
- Consumes: everything above.
- Produces: a green `npm run check`.

- [ ] **Step 1: Set up the environment**

Run: `cd web && npm ci`, then from the repo root `pip install -e ".[docs]"` and build the Rust extension. The gate cannot run without these, and this is the first task that needs the full build rather than a typecheck.

- [ ] **Step 2: Retune the Shiki themes**

In `next.config.mjs`, replace `github-light` / `github-dark-default` with a pair that sits on cream and near-black. The spec leaves the exact pair to the builder; keep the existing comment's argument — one theme per colour scheme, not one forced through a filter.

- [ ] **Step 3: Run the full gate**

Run: `cd web && npm run check && node scripts/check-seo.mjs`
Expected: PASS — regenerate, build, link-check, contrast, discovery and machine-readable prose. All five generators must produce output; no orphan pages; no dangling anchors; the sitemap lists nothing that 404s.

- [ ] **Step 4: Look at it**

Open a `Diagnostic`, a `RunReport` and a `Stats` row in both modes. Check specifically that `notice` does not read as brand orange. If it does, apply the spec's documented fallback — a neutral-warm rule with no fill — and say so in `identity.mdx`. Check the 404, the favicon on a light and a dark tab strip, and one printed page.

- [ ] **Step 5: Update the changelog**

Under `[Unreleased] / ### Changed`, describe the identity and the rebuild in the changelog's established voice: what changed, and why each divergence was chosen. Note that `VOL. 02 — THE ENGINE` ships as a knowing placeholder pending the author's volume number. **No AI attribution.**

- [ ] **Step 6: Commit**

```bash
git add web/next.config.mjs CHANGELOG.md
git commit -m "chore: retune code themes and record the rebrand"
```
