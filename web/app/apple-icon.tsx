import { ImageResponse } from 'next/og'

/*
 * The home-screen icon, rendered once at build time.
 *
 * Next registers this as <link rel="apple-touch-icon"> by itself, which is why
 * metadata.icons in layout.tsx deliberately has no `apple` entry.
 *
 * It draws the SMALL variant of the mark (geometry of public/favicon.svg):
 * thicker slabs and a thicker tie, because the display variant's 3.5-unit
 * slabs thin out to hairlines at this size. iOS rounds the corners itself, so
 * the plate is full bleed. Colours are literals: next/og reads no CSS.
 */
export const dynamic = 'force-static'
export const size = { width: 180, height: 180 }
export const contentType = 'image/png'

export default function AppleIcon() {
  return new ImageResponse(
    (
      <div
        style={{
          width: '100%',
          height: '100%',
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'center',
          background: '#0A0A0B',
        }}
      >
        <svg width="132" height="132" viewBox="0 0 100 100" fill="none">
          <g fill="#F5F5F0">
            <rect x="26" y="10" width="16" height="76" />
            <rect x="18" y="10" width="32" height="6" />
            <rect x="15" y="80" width="38" height="6" />
            <rect x="60" y="44" width="16" height="42" />
            <rect x="52" y="44" width="32" height="6" />
            <rect x="49" y="80" width="38" height="6" />
            <circle cx="68" cy="26" r="9" />
          </g>
          <polygon points="26,58 76,52 76,66 26,72" fill="#E36414" />
        </svg>
      </div>
    ),
    size,
  )
}
