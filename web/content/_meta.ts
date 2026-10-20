import type { MetaRecord } from 'nextra'

/*
 * The navigation. Order is deliberate and mirrors how someone arrives:
 * the failure they are searching for, then the ideas, then the how-to, then
 * the internals, then the specification.
 */
const meta: MetaRecord = {
  index: { title: 'Home', theme: { sidebar: false, toc: false, pagination: false, breadcrumb: false, layout: 'full' } },
  'silent-failures': { title: 'Silent failures' },
  'case-studies': { title: 'In use' },
  notebooks: { title: 'Notebooks' },
  concepts: { title: 'Concepts' },
  guide: { title: 'Guide' },
  architecture: { title: 'Architecture' },
  design: { title: 'Design' },
  api: { title: 'API reference' },
  licence: { title: 'Licence' },
}

export default meta
