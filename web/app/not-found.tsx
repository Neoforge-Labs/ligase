import type { Metadata } from 'next'
import Link from 'next/link'

/*
 * The layout's canonical is './', resolved against the route being rendered.
 * For the 404 that route is Next's internal /_not-found, which would leak
 * into public HTML. The export serves this page at /404/ (and as 404.html), so
 * that is the address it declares. It stays noindex, so it is never a search
 * result and never in the sitemap; the canonical only has to be true.
 */
export const metadata: Metadata = {
  title: 'Page not found',
  alternates: { canonical: '/404/' },
}

/*
 * The 404 is the one place the mark is allowed to say no.
 *
 * It is the same two letterforms as the mark, the l and the i, but the tie
 * that binds them is cut in two and the halves are offset, drawn in the
 * refusal colour instead of the brand orange. Ligase's whole job is to refuse
 * a connection that does not hold, and this is that refusal, drawn.
 *
 * Geometry is public/icon.svg's. The two halves of the tie are the original
 * slanted polygon cut at x=47 and x=55, the right-hand half dropped 7 units.
 * Static export turns this into out/404.html, which GitHub Pages serves for
 * any unknown path. It is exempt from scripts/check-links.mjs's orphan rule,
 * so it needs no navigation entry.
 */
export default function NotFound() {
  return (
    <main
      style={{
        display: 'flex',
        flexDirection: 'column',
        alignItems: 'center',
        gap: '1.25rem',
        padding: '6rem 1rem',
        textAlign: 'center',
      }}
    >
      <svg
        width="120"
        height="120"
        viewBox="0 0 100 100"
        fill="none"
        aria-hidden="true"
        style={{ color: 'var(--lg-ink)' }}
      >
        <g fill="currentColor">
          <rect x="30" y="14" width="10" height="68" />
          <rect x="24" y="14" width="22" height="3.5" />
          <rect x="22" y="78.5" width="26" height="3.5" />
          <rect x="62" y="42" width="10" height="40" />
          <rect x="56" y="42" width="22" height="3.5" />
          <rect x="54" y="78.5" width="26" height="3.5" />
          <circle cx="67" cy="28" r="6" />
        </g>
        <g fill="var(--lg-refuse)">
          <polygon points="30,56 47,52.8 47,61.8 30,65" />
          <polygon points="55,58.2 72,55 72,64 55,67.2" />
        </g>
      </svg>
      <h1
        style={{
          margin: 0,
          fontFamily: 'var(--lg-font-display)',
          fontWeight: 400,
          fontSize: '2rem',
          letterSpacing: '-0.02em',
          color: 'var(--lg-ink)',
        }}
      >
        Nothing is bound to this address.
      </h1>
      <p style={{ margin: 0, color: 'var(--lg-ink-muted)' }}>
        The page you asked for does not exist, or it has moved.
      </p>
      <Link href="/">Back to the start</Link>
    </main>
  )
}
