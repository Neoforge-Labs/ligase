import type { MetaRecord } from 'nextra'

/*
 * The internals, outside in: where things live, what a type is, the surface
 * that only looks at one, how the graph is built from it, the steps the
 * project ships ready-built, how it runs, what is cached, how a step is run
 * twice and judged, how outputs are canonicalised before hashing, how a
 * conversion crosses an axis, where an input is fetched from, what a step
 * noticed about its own work, what is recorded, the one domain interpreter,
 * and how the site is dressed.
 */
const meta: MetaRecord = {
  workspace: 'Workspace',
  'type-system': 'Type system',
  inspect: 'Inspect',
  graph: 'Graph',
  'step-library': 'Step library',
  execution: 'Execution',
  cache: 'Cache',
  store: 'Store',
  audit: 'Audit',
  canonicalisation: 'Canonicalisation',
  conversions: 'Conversions',
  remotes: 'Remote inputs',
  notes: 'Notes',
  provenance: 'Provenance',
  hivdb: 'Resistance interpretation',
  identity: 'Visual identity',
}

export default meta
