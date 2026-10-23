import type { MetaRecord } from 'nextra'

/*
 * Explanation, in the order the ideas build on each other. Each page opens
 * with the question it answers.
 *
 * Sharing a cache comes after the two pages about where data comes from,
 * because it is the same content-addressing idea turned outwards: the page
 * assumes you know what a cache hit is and what a recorded body is for.
 */
const meta: MetaRecord = {
  'types-and-dimensions': 'Types and dimensions',
  pipelines: 'Pipelines and the graph',
  caching: 'Running, and not running',
  'semantic-hashing': 'What a file means',
  provenance: 'How was this produced?',
  identifiers: 'Identifier spaces',
  remotes: 'Where the data came from',
  'record-and-replay': 'Recording a live answer',
  'remote-store': 'Sharing a cache',
  notes: 'What a result rests on',
  'resistance-interpretation': 'Resistance interpretation',
}

export default meta
