'use client'

/*
 * Reports soft navigations to the container.
 *
 * Nextra moves between pages without a document load, so the container's All
 * Pages trigger fires once per visit however many pages a reader goes on to
 * open. Without this bridge every page after the landing one would be missing
 * from the counts altogether: only landing paths would ever appear, and a page
 * nobody arrives on directly would read as never opened. The counts are
 * per-page and that is all they are — consent is denied by default, so there is
 * no client ID and no per-reader journey to recover from them.
 *
 * This lives in its own file rather than in Analytics.tsx because 'use client'
 * is module-scoped in the App Router. Putting it there would turn
 * GoogleTagManager, whose inline scripts must be rendered on the server and in
 * a fixed order, into a client component as well, and would make GTM_ID a
 * client reference to anything on the server that imports it.
 *
 * Three decisions, each one a way the count goes quietly wrong:
 *
 *   - usePathname excludes the hash and the query, so clicking an "On this
 *     page" anchor is not a pageview. Reading location.href instead would count
 *     every table-of-contents click as a new page.
 *   - The first path is skipped. The GA4 configuration tag sends its own
 *     page_view when it initialises, which covers the landing path, and the
 *     landing path is typically most of the traffic, so reporting it again here
 *     would double the largest number on the dashboard. That tag's automatic
 *     pageview therefore has to stay enabled: suppress it and the landing path
 *     is reported by nobody. The ref is seeded true and flipped on the first
 *     run of the effect. (React StrictMode runs the effect twice on mount, so
 *     `next dev` does push the landing path; dev loads no container, so nothing
 *     is counted.)
 *   - The path is reported with the trailing slash the export serves
 *     (trailingSlash: true), so the pushed value agrees with the URL the
 *     browser is on. Without it, anything reading page_path files one page
 *     under two rows.
 *
 * dataLayer is initialised before the push because this effect can run before
 * the loader has: a push against an undefined array throws, and a throw here
 * loses every pageview after it, not just one.
 */
import { usePathname } from 'next/navigation'
import { useEffect, useRef } from 'react'

declare global {
  interface Window {
    dataLayer?: unknown[]
  }
}

export function AnalyticsPageview(): null {
  const pathname = usePathname()
  const first = useRef(true)

  useEffect(() => {
    if (first.current) {
      first.current = false
      return
    }
    window.dataLayer = window.dataLayer || []
    window.dataLayer.push({
      event: 'page_view',
      page_path: pathname.endsWith('/') ? pathname : `${pathname}/`,
    })
  }, [pathname])

  return null
}
