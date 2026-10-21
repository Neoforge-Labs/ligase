import type { Metadata } from 'next'
import { Playfair_Display, Source_Serif_4 } from 'next/font/google'
import { GeistMono } from 'geist/font/mono'
import { GeistSans } from 'geist/font/sans'
import { Footer, Layout, Navbar } from 'nextra-theme-docs'
import { Head } from 'nextra/components'
import { getPageMap } from 'nextra/page-map'
import 'nextra-theme-docs/style.css'
import './globals.css'
import { Mark } from '../components/Mark'

/*
 * Three typefaces, one role each.
 *
 *   Geist Sans    headings and chrome
 *   Source Serif  body prose
 *   Geist Mono    code, and every word the engine says
 *
 * The serif body is the deliberate break from the previous site. This is
 * 52,000 words of technical prose written for people who read papers, and a
 * serif at a 72-character measure is how that is normally set.
 */
const sourceSerif = Source_Serif_4({
  subsets: ['latin'],
  display: 'swap',
  variable: '--font-source-serif',
  axes: ['opsz'],
})

/*
 * The display face, used only for the wordmark and the largest headings.
 * Its variable name is what --lg-font-display in tokens.css reads.
 */
const playfair = Playfair_Display({
  subsets: ['latin'],
  weight: ['300', '400'],
  style: ['normal', 'italic'],
  display: 'swap',
  variable: '--font-playfair',
})

export const metadata: Metadata = {
  metadataBase: new URL('https://ligase.neoforgelabs.tech'),
  title: {
    default: 'Ligase — type-safe, reproducible computational biology pipelines',
    template: '%s — Ligase',
  },
  description:
    'Biological data carries dimensions — organism, reference build, coordinate system, sample set — and Ligase refuses to connect steps whose dimensions disagree. A Rust core with a Python surface.',
  applicationName: 'Ligase',
  // Only the small variant is declared: icon.svg's thin slabs collapse at 16px.
  icons: {
    icon: [{ url: '/favicon.svg', type: 'image/svg+xml' }],
    shortcut: '/favicon.svg',
  },
  authors: [{ name: 'Kelyn Njeri' }],
  openGraph: {
    type: 'website',
    siteName: 'Ligase',
    url: 'https://ligase.neoforgelabs.tech',
  },
  twitter: { card: 'summary_large_image' },
}

const navbar = (
  <Navbar logo={<Mark />} projectLink="https://github.com/Neoforge-Labs/ligase" />
)

const footer = (
  <Footer>
    <div className="lg-footer">
      <p>
        <strong>Ligase</strong> — a Rust core with a Python surface, for pipelines that
        are correct by construction.
      </p>
      <p>
        Dual-licensed <a href="/licence/">MIT or Apache-2.0</a>. Copyright © 2026 Kelyn
        Paul Njeri, NeoForge Labs.
      </p>
    </div>
  </Footer>
)

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    <html
      lang="en"
      dir="ltr"
      suppressHydrationWarning
      className={`${GeistSans.variable} ${GeistMono.variable} ${sourceSerif.variable} ${playfair.variable}`}
    >
      {/*
        Nextra derives its own accent (links, active nav, focus rings) and page
        background from these, so they are set here rather than overridden in
        our stylesheet. One source of truth beats a layer of `!important`.
      */}
      <Head
        color={{
          hue: { light: 23, dark: 23 },
          saturation: { light: 89, dark: 84 },
          lightness: { light: 35, dark: 48 },
        }}
        backgroundColor={{ light: '#f5f5f0', dark: '#0a0a0b' }}
      >
        <meta name="theme-color" content="#f5f5f0" media="(prefers-color-scheme: light)" />
        <meta name="theme-color" content="#0a0a0b" media="(prefers-color-scheme: dark)" />
      </Head>
      <body>
        <Layout
          navbar={navbar}
          footer={footer}
          pageMap={await getPageMap()}
          docsRepositoryBase="https://github.com/Neoforge-Labs/ligase/tree/main/web/content"
          editLink="Edit this page on GitHub"
          // The project has no issue-triage capacity yet; a feedback link that
          // goes nowhere is worse than none.
          feedback={{ content: null }}
          sidebar={{ defaultMenuCollapseLevel: 1, autoCollapse: false, toggleButton: true }}
          toc={{ title: 'On this page', backToTop: 'Back to top' }}
          darkMode
          // Dark is what a visitor sees on arrival; light stays one toggle away.
          nextThemes={{ defaultTheme: 'dark' }}
        >
          {children}
        </Layout>
      </body>
    </html>
  )
}
