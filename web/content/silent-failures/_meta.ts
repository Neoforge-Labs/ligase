import type { MetaRecord } from 'nextra'

// Ordered by how often the failure happens, not by severity. Titles are the
// symptom rather than the mechanism, because that is what people search for.
const meta: MetaRecord = {
  index: 'What goes wrong silently',
  'contig-naming': 'My BED file intersects nothing',
  'wrong-reference-build': 'My variant positions are wrong',
  'coordinate-systems': 'My intervals are off by one',
  'shuffled-samples': 'My sample labels look swapped',
  'mixed-identifiers': 'My gene-set overlap is empty',
  'mixed-sources': 'My target ranking changed on its own',
  'mixed-organism': 'My pathogen results include host reads',
  'reference-frame': 'My resistance report names the wrong gene',
  'contig-order': 'GATK rejects my sequence dictionary',
}

export default meta
