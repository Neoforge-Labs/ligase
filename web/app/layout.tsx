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
 *
 * Playfair Display is a 400-900 family: 400 is its lightest real weight, and
 * the Google Fonts API refuses wght@300 outright. (The lighter, differently
 * drawn face with a 300 is the separate "Playfair" family.) Every rule that
 * sets the display face must therefore ask for a weight listed here, which
 * scripts/check-furniture.mjs enforces.
 */
const playfair = Playfair_Display({
  subsets: ['latin'],
  weight: ['400'],
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
    // Next links a file-convention icon only when `icons` is unset, so the
    // Apple icon (app/apple-icon.png/route.tsx) has to be named here.
    apple: [{ url: '/apple-icon.png', sizes: '180x180', type: 'image/png' }],
  },
  authors: [{ name: 'Kelyn Njeri' }],
  // Relative, so each page resolves it against metadataBase and its own path
  // and declares itself, with the trailing slash the export serves.
  alternates: { canonical: './' },
  openGraph: {
    type: 'website',
    siteName: 'Ligase',
    url: 'https://ligase.neoforgelabs.tech',
    // Rendered at build time by app/opengraph-image.png/route.tsx.
    images: [
      {
        url: '/opengraph-image.png',
        width: 1200,
        height: 630,
        alt: 'Ligase: type-safe, reproducible computational biology pipelines',
      },
    ],
  },
  twitter: {
    card: 'summary_large_image',
    images: ['/opengraph-image.png'],
  },
}

/*
 * Structured data, and only what can be checked against the repository. The
 * project is pre-alpha and has no outside users, so there is deliberately no
 * rating, review, download or interaction count here, and none may be added
 * until it is true (scripts/check-seo.mjs fails on them).
 */
const jsonLd = {
  '@context': 'https://schema.org',
  '@type': 'SoftwareApplication',
  name: 'Ligase',
  description:
    'A pre-alpha Rust core with a Python surface for computational biology pipelines. Biological data types carry dimensions, and the engine refuses to connect steps whose dimensions disagree.',
  url: 'https://ligase.neoforgelabs.tech/',
  applicationCategory: 'DeveloperApplication',
  operatingSystem: 'Linux, macOS, Windows',
  programmingLanguage: ['Rust', 'Python'],
  softwareVersion: '0.4.0',
  sameAs: 'https://github.com/Neoforge-Labs/ligase',
  license: ['https://spdx.org/licenses/MIT.html', 'https://spdx.org/licenses/Apache-2.0.html'],
  author: { '@type': 'Person', name: 'Kelyn Njeri' },
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
      <p className="lg-mono">◉ WESTLANDS, NAIROBI · MMXXVI</p>
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
        {/*
          One unconditional value, placed first. The site opens dark whatever
          the OS prefers (`defaultTheme: 'dark'` below), so a light variant
          keyed to prefers-color-scheme gives an OS-light visitor cream
          browser chrome around a near-black page. Nextra's <Head> still emits
          its own light/dark pair after this; a browser takes the first
          theme-color whose media matches, which is this one, and
          check-furniture --built asserts that it stays first. A light variant
          would only be right if it tracked the applied theme, which needs
          script, so a visitor who has chosen light keeps dark chrome.
        */}
        <meta name="theme-color" content="#0a0a0b" />
      </Head>
      <body className="lg-grain">
        <script
          type="application/ld+json"
          // `<` is escaped so no string in the block can close the script tag.
          dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, '\\u003c') }}
        />
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
