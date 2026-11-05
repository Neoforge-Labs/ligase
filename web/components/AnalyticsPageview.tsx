'use client'

/*
 * Reports soft navigations to the container.
 *
 * Nextra moves between pages without a document load, so the container's All
 * Pages trigger fires once per visit however many pages a reader goes on to
 * open. Without this bridge the dashboard would show every visit as a one-page
 * visit, which is a silent undercount of exactly the thing the site wants to
 * know: whether people read past the page they landed on.
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
 *   - The first path is skipped. The container reports the landing page itself
 *     when it loads, and the landing page is most of the traffic, so reporting
 *     it again would double the largest number on the dashboard. The ref is
 *     seeded true and flipped on the first run of the effect. (React
 *     StrictMode runs the effect twice on mount, so `next dev` does push the
 *     landing path; dev loads no container, so nothing is counted.)
 *   - The path is reported with the trailing slash the export serves
 *     (trailingSlash: true). Reported without it, one page would appear as two
 *     rows, one from the container and one from this bridge.
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
