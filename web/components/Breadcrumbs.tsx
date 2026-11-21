import type { PageMapItem } from 'nextra'
import { getPageMap } from 'nextra/page-map'

/*
 * BreadcrumbList for the page being rendered, beside the SoftwareApplication
 * the root layout declares.
 *
 * The theme already draws a visible breadcrumb above every page, so this is
 * markup for a trail a reader can see rather than one invented for a crawler.
 * What it does not do is invent URLs. Seven of the site's eleven sections are
 * folders with no index.mdx -- concepts/, guide/, guide/steps/, start/,
 * reference/, project/ and project/architecture/ -- so the visible trail shows
 * their names as plain text, and there is nothing to link them to. Google
 * requires `item` on every element of a BreadcrumbList except the last, so a
 * section with no page is left out of the trail rather than pointed at a 404.
 * For most pages that leaves `Ligase > <page>`; where an ancestor is a real
 * page (silent-failures/, notebooks/, reference/api/, project/in-use/) it
 * appears between them. Give a section an index.mdx and its crumb starts
 * appearing here with no change to this file.
 *
 * Read off the same page map as the sidebar and the sitemap, so a page cannot
 * be in the navigation and absent from its own breadcrumb.
 */

const ORIGIN = 'https://ligase.neoforgelabs.tech'

type Node = { label: string; isPage: boolean }

/** The `_meta.ts` record for a level, which is where its nav labels live. */
function labelsOf(items: readonly PageMapItem[]): Record<string, unknown> {
  const record = items.find((item) => 'data' in item) as { data?: Record<string, unknown> } | undefined
  return record?.data ?? {}
}

/**
 * Every route in the page map, with the label its parent's `_meta.ts` gives
 * it and whether a page is actually served there. A folder has a `route`
 * whether or not it has an index.mdx; the index arrives as a child of the
 * folder carrying the same route, which is the only thing that makes it a
 * page. The folder's own label is kept in that case, because `index` is what
 * the child is called in its level's meta and the section's name is what the
 * breadcrumb wants.
 */
function nodesOf(
  items: readonly PageMapItem[],
  into: Map<string, Node> = new Map(),
): Map<string, Node> {
  const labels = labelsOf(items)
  for (const item of items) {
    if ('data' in item) continue
    const node = item as {
      name?: string
      route?: string
      title?: string
      frontMatter?: { title?: string }
      children?: readonly PageMapItem[]
    }
    const meta = node.name ? labels[node.name] : undefined
    const label =
      (typeof meta === 'string' ? meta : undefined) ??
      (meta && typeof meta === 'object' && 'title' in meta ? String((meta as { title: unknown }).title) : undefined) ??
      node.frontMatter?.title ??
      node.title ??
      node.name ??
      ''
    if (node.children) {
      if (node.route) into.set(node.route, { label, isPage: false })
      nodesOf(node.children, into)
    } else if (node.route) {
      const existing = into.get(node.route)
      into.set(node.route, { label: existing?.label ?? label, isPage: true })
    }
  }
  return into
}

/** The crumbs for a route: the site, the ancestors that are pages, the page. */
function trailFor(route: string, nodes: Map<string, Node>) {
  const crumbs = [{ name: 'Ligase', url: `${ORIGIN}/` }]
  const segments = route.split('/').filter(Boolean)
  let path = ''
  for (const segment of segments) {
    path += `/${segment}`
    const node = nodes.get(path)
    // A section with no index.mdx has no address to offer, so it is skipped.
    if (node?.isPage) crumbs.push({ name: node.label, url: `${ORIGIN}${path}/` })
  }
  return crumbs
}

export async function Breadcrumbs({ mdxPath }: { mdxPath?: string[] }) {
  // The front page is the root of the trail, so it has no breadcrumb.
  if (!mdxPath?.length) return null

  const crumbs = trailFor(`/${mdxPath.join('/')}`, nodesOf(await getPageMap()))
  if (crumbs.length < 2) return null

  const jsonLd = {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: crumbs.map((crumb, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: crumb.name,
      item: crumb.url,
    })),
  }

  return (
    <script
      type="application/ld+json"
      // `<` is escaped so no string in the block can close the script tag,
      // as in the layout's own JSON-LD.
      dangerouslySetInnerHTML={{ __html: JSON.stringify(jsonLd).replace(/</g, '\\u003c') }}
    />
  )
}
