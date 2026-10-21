import type { MetaRecord } from 'nextra'

/*
 * The internals, outside in: where things live, what a type is, how the
 * graph is built from it, how it runs, what is cached, how outputs are
 * canonicalised before hashing, what is recorded, and the one domain
 * interpreter.
 */
const meta: MetaRecord = {
  workspace: 'Workspace',
  'type-system': 'Type system',
  graph: 'Graph',
  execution: 'Execution',
  cache: 'Cache',
  canonicalisation: 'Canonicalisation',
  provenance: 'Provenance',
  hivdb: 'Resistance interpretation',
}

export default meta
