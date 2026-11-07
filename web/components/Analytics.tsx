/*
 * Google Tag Manager, and the one place the site talks to a third party.
 *
 * Two inline scripts, rendered as the first thing in <body>. The order is the
 * point. Consent Mode only governs a container that was told its defaults
 * before it started, so the consent default has to run to completion before
 * the loader is even requested; a container that loads first reads no defaults
 * and treats every visitor as having agreed. Both scripts are inline and
 * synchronous for that reason, and neither goes through next/script, whose
 * strategies are free to reorder them.
 *
 * All four signals default to denied. Nothing on this site grants them: there
 * is no banner, so until one exists the container collects only what Consent
 * Mode permits without consent, and a visitor is never counted as having
 * agreed to something they were not asked.
 *
 * There is deliberately no <noscript> iframe. The canonical snippet carries one
 * to count a visitor with JavaScript off, but it cannot do that here: GA4 tags
 * are JavaScript and do not fire inside ns.html, and GA4 is the only tag this
 * container is meant to hold, so that visitor goes uncounted either way. What
 * the iframe would add is a third-party request made before the consent default
 * — itself JavaScript — has run, on a site whose whole posture is deny by
 * default. Removing it loses no measurement that was ever achievable.
 *
 * check-furniture --built reads GTM_ID out of this file's text and asserts that
 * every exported page carries the consent default before one loader. A tag that
 * silently stopped shipping is the failure shape this project exists to refuse,
 * so the build says so rather than the dashboard going quiet. Keep the single
 * quotes in the script bodies: that assertion matches on them.
 */
import type { ReactNode } from 'react'

export const GTM_ID = 'GTM-NV6SCC4L'

const CONSENT_DEFAULT_JS = `window.dataLayer=window.dataLayer||[];function gtag(){dataLayer.push(arguments);}gtag('consent','default',{ad_storage:'denied',ad_user_data:'denied',ad_personalization:'denied',analytics_storage:'denied'});`

/*
 * The canonical container snippet. It pushes gtm.start before the container
 * script is requested, which is what lets the container measure its own load
 * time and fire its All Pages trigger.
 */
const GTM_LOADER_JS = `(function(w,d,s,l,i){w[l]=w[l]||[];w[l].push({'gtm.start':new Date().getTime(),event:'gtm.js'});var f=d.getElementsByTagName(s)[0],j=d.createElement(s),dl=l!='dataLayer'?'&l='+l:'';j.async=true;j.src='https://www.googletagmanager.com/gtm.js?id='+i+dl;f.parentNode.insertBefore(j,f);})(window,document,'script','dataLayer','${GTM_ID}');`

/*
 * Production only, which in practice means: not under `next dev`, where the
 * author's own reloads would otherwise be counted. `next build` is a
 * production build, so a local build does carry the tag — which is what lets
 * check-furniture --built find the real thing in out/.
 */
export function GoogleTagManager(): ReactNode {
  if (process.env.NODE_ENV !== 'production') return null
  return (
    <>
      <script dangerouslySetInnerHTML={{ __html: CONSENT_DEFAULT_JS }} />
      <script dangerouslySetInnerHTML={{ __html: GTM_LOADER_JS }} />
    </>
  )
}
