import type { MetaRecord } from 'nextra'

/*
 * The internals, outside in: where things live, what a type is, the surface
 * that only looks at one, how the graph is built from it, how it runs, what
 * is cached, how outputs are canonicalised before hashing, what a step
 * noticed about its own work, what is recorded, the one domain interpreter,
 * and how the site is dressed.
 */
const meta: MetaRecord = {
  workspace: 'Workspace',
  'type-system': 'Type system',
  inspect: 'Inspect',
  graph: 'Graph',
  execution: 'Execution',
  cache: 'Cache',
  canonicalisation: 'Canonicalisation',
  conversions: 'Conversions',
  notes: 'Notes',
  provenance: 'Provenance',
  hivdb: 'Resistance interpretation',
  identity: 'Visual identity',
}

export default meta
