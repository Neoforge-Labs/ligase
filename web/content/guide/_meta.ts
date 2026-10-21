import type { MetaRecord } from 'nextra'

/*
 * One task per page, in the order someone meets them: write a pipeline,
 * tell it what the files cannot say about themselves, run it, write it up,
 * then the two things that come up once it is running, then the HIV-1
 * interpreter.
 *
 * Hand-written rather than taken from mkdocs.yml: these pages are now
 * authored as MDX, and several were retitled to name the task rather than
 * the mechanism.
 */
const meta: MetaRecord = {
  'building-a-pipeline': 'Building a pipeline',
  'declaring-dimensions': 'Declaring dimensions',
  'running-a-pipeline': 'Running a pipeline',
  provenance: 'Writing up a pipeline',
  'semantic-hashing': 'Checking that a step repeats',
  notes: 'Fixing a noticed line',
  'resistance-interpretation': 'Interpreting resistance',
}

export default meta
