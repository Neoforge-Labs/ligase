import type { MetaRecord } from 'nextra'

/*
 * One task per page, in the order someone meets them: look at files you
 * already have, write a pipeline, tell it what the files cannot say about
 * themselves, run it, write it up, then the two things that come up once it
 * is running, then the HIV-1 interpreter.
 *
 * Inspecting comes first because it is the one task that needs nothing
 * written: no pipeline, no decorator, no commitment.
 *
 * Several pages are titled after the task rather than the mechanism that
 * serves it.
 *
 * The step library sits immediately after building a pipeline, because it is
 * what most of a first pipeline is made of: a reader who has just met
 * `@lg.step` should find out that the four commonest steps are already
 * written before being shown how to write one.
 *
 * Binding a number sits immediately after writing a pipeline up, because it
 * is the next thing you do with a write-up and the page is read with a
 * manuscript open: the claims are printed in the same report the previous
 * page explains.
 *
 * Ranked lists sit after crossing identifier spaces because the page assumes
 * it: a ranking is a gene list that remembers its order, and the two
 * refusals it meets are the ones that page introduces.
 *
 * Knowledge graphs come after ranked lists, because the page assumes both of
 * them: a graph names entities the way a list does, and the refusal that is
 * new to it is the one about how the graph was assembled.
 *
 * Compound sets come after knowledge graphs, last of the pages about a kind
 * that holds no sequence, because the page assumes what the other two teach:
 * that a dimension can be about what a name means, and that the engine
 * refuses rather than repairs.
 *
 * The two remote pages come after them and in that order, because a pinned
 * remote is the simpler promise and a recorded query is explained by
 * contrast with it. Pointing a lab at a bucket follows both, because what a
 * thin bundle gives up is a recorded query's one guarantee.
 *
 * Running on a cluster follows pointing a lab at a bucket, because a store
 * every machine can see is the thing it needs before it can start: the page
 * opens by assuming you have one.
 *
 * Declaring a stochastic step follows checking that a step repeats, because
 * what the declaration changes is what that page's audit is allowed to say.
 */
const meta: MetaRecord = {
  'inspecting-files': 'Inspecting files you have',
  'building-a-pipeline': 'Building a pipeline',
  steps: { title: 'Steps you do not have to write' },
  'declaring-dimensions': 'Declaring dimensions',
  'running-a-pipeline': 'Running a pipeline',
  provenance: 'Writing up a pipeline',
  claims: 'Binding a number you published',
  'mapping-identifiers': 'Crossing identifier spaces',
  'ranked-lists': 'Comparing ranked lists',
  'knowledge-graphs': 'Comparing knowledge graphs',
  molecules: 'Comparing compound sets',
  remotes: 'Fetching your inputs',
  'recorded-queries': 'Recording a live query',
  'remote-store': 'Pointing a lab at a bucket',
  'cluster-execution': 'Running on a cluster',
  'semantic-hashing': 'Checking that a step repeats',
  'stochastic-steps': 'Declaring a stochastic step',
  notes: 'Fixing a noticed line',
  'resistance-interpretation': 'Interpreting resistance',
}

export default meta
