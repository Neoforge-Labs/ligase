import type { MetaRecord } from 'nextra'

/*
 * The internals, outside in: where things live, what a type is, how the
 * graph is built from it, how it runs, what is cached, how outputs are
 * canonicalised before hashing, what a step noticed about its own work,
 * what is recorded, the one domain interpreter, and how the site is dressed.
 */
const meta: MetaRecord = {
  workspace: 'Workspace',
  'type-system': 'Type system',
  graph: 'Graph',
  execution: 'Execution',
  cache: 'Cache',
  canonicalisation: 'Canonicalisation',
  notes: 'Notes',
  provenance: 'Provenance',
  hivdb: 'Resistance interpretation',
  identity: 'Visual identity',
}

export default meta
