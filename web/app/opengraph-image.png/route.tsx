import { ImageResponse } from 'next/og'

/*
 * The social card, rendered once at build time.
 *
 * This is a route handler in a folder named opengraph-image.png, not the
 * opengraph-image.tsx file convention, on purpose. Under `output: 'export'`
 * the file convention writes `out/opengraph-image` with no extension, which
 * GitHub Pages types by extension and so would likely serve as
 * application/octet-stream, and scrapers check Content-Type. A route handler
 * whose path ends in .png is exported as `out/opengraph-image.png`. The
 * convention would also hash the URL and link it itself; here layout.tsx
 * names it explicitly.
 *
 * `output: 'export'` has no server to render it per request, so the route
 * must be declared static.
 *
 * next/og resolves neither CSS variables nor next/font bindings, so every
 * colour is a literal (the same values as tokens.css and public/icon.svg) and
 * the display face is fetched and handed over through the `fonts` option.
 */
export const dynamic = 'force-static'
const size = { width: 1200, height: 630 }

const PLATE = '#0A0A0B'
const CREAM = '#F5F5F0'
const ORANGE = '#E36414'
const MUTED = '#A1A1AA'

const DESCRIPTION = 'Pipelines that refuse to connect data whose dimensions disagree.'

type Font = {
  name: string
  data: ArrayBuffer
  style: 'normal' | 'italic'
  weight: 400
}

/*
 * Playfair Display, upright and italic, subset by Google to exactly the
 * glyphs this card uses so the files stay a few kilobytes against next/og's
 * 500KB budget. Asking Google's CSS endpoint without a browser user agent is
 * what makes it answer with TrueType, which is the only format Satori reads.
 *
 * A build must never break on a font CDN. Any failure returns an empty list,
 * and next/og then draws with the face it bundles (a sans, since it ships no
 * serif), which is a worse card but a card.
 */
async function loadFonts(text: string): Promise<Font[]> {
  try {
    const glyphs = encodeURIComponent([...new Set(text)].join(''))
    const cssUrl =
      'https://fonts.googleapis.com/css2?family=Playfair+Display:ital,wght@0,400;1,400' +
      `&text=${glyphs}`
    const cssResponse = await fetch(cssUrl)
    if (!cssResponse.ok) return []
    const css = await cssResponse.text()

    const fonts: Font[] = []
    for (const block of css.match(/@font-face\s*{[^}]*}/g) ?? []) {
      const url = block.match(/src:\s*url\(([^)]+)\)\s*format\('truetype'\)/)?.[1]
      const style = /font-style:\s*italic/.test(block) ? 'italic' : 'normal'
      if (!url) continue
      const fontResponse = await fetch(url)
      if (!fontResponse.ok) return []
      fonts.push({
        name: 'Playfair Display',
        data: await fontResponse.arrayBuffer(),
        style,
        weight: 400,
      })
    }
    // Both cuts or neither: a card with an upright "ase" is a different card.
    return fonts.length === 2 ? fonts : []
  } catch {
    // Deliberate: a font CDN outage degrades the card to next/og's bundled
    // sans rather than failing the build. check-assets cannot see the
    // difference, so a degraded card is a visual regression to look for.
    return []
  }
}

/* The display mark, geometry from public/icon.svg, as Satori-friendly SVG. */
function DisplayMark({ px }: { px: number }) {
  return (
    <svg width={px} height={px} viewBox="0 0 100 100" fill="none">
      <g fill={CREAM}>
        <rect x="30" y="14" width="10" height="68" />
        <rect x="24" y="14" width="22" height="3.5" />
        <rect x="22" y="78.5" width="26" height="3.5" />
        <rect x="62" y="42" width="10" height="40" />
        <rect x="56" y="42" width="22" height="3.5" />
        <rect x="54" y="78.5" width="26" height="3.5" />
        <circle cx="67" cy="28" r="6" />
      </g>
      <polygon points="30,56 72,48 72,57 30,65" fill={ORANGE} />
    </svg>
  )
}

export async function GET() {
  const fonts = await loadFonts(`Ligase${DESCRIPTION}`)
  const serif = fonts.length > 0 ? 'Playfair Display' : 'sans-serif'

  return new ImageResponse(
    (
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          flexDirection: 'column',
          justifyContent: 'center',
          background: PLATE,
          padding: '0 96px',
        }}
      >
        <div style={{ display: 'flex', alignItems: 'center' }}>
          <DisplayMark px={240} />
          <div
            style={{
              display: 'flex',
              marginLeft: 40,
              fontFamily: serif,
              fontSize: 188,
              letterSpacing: '-0.02em',
              lineHeight: 1,
              color: CREAM,
            }}
          >
            <span>Lig</span>
            <span style={{ fontStyle: 'italic', color: ORANGE }}>ase</span>
          </div>
        </div>
        <div
          style={{ width: 48, height: 3, background: ORANGE, marginTop: 56, marginBottom: 32 }}
        />
        <div style={{ fontFamily: serif, fontSize: 32, lineHeight: 1.25, color: MUTED }}>
          {DESCRIPTION}
        </div>
      </div>
    ),
    { ...size, fonts },
  )
}
