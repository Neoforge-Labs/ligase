import type { MetaRecord } from 'nextra'

/*
 * Explanation, in the order the ideas build on each other. Each page opens
 * with the question it answers.
 *
 * Stochastic steps come after the two remote pages because the argument is
 * the same one a third time -- what, exactly, can be promised -- and the
 * page leans on the audit vocabulary that semantic hashing introduces.
 */
const meta: MetaRecord = {
  'types-and-dimensions': 'Types and dimensions',
  pipelines: 'Pipelines and the graph',
  caching: 'Running, and not running',
  'step-library': 'What a curated step is',
  'semantic-hashing': 'What a file means',
  provenance: 'How was this produced?',
  identifiers: 'Identifier spaces',
  remotes: 'Where the data came from',
  'record-and-replay': 'Recording a live answer',
  'stochastic-steps': 'Steps that draw on randomness',
  notes: 'What a result rests on',
  'resistance-interpretation': 'Resistance interpretation',
}

export default meta
