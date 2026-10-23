import type { MetaRecord } from 'nextra'

/*
 * One page per library step, in the order a pipeline meets them: align,
 * align the other way, sort something that came from elsewhere, call.
 *
 * Titled after the step rather than after the tool, because the tool is
 * the part a reader already knows.
 */
const meta: MetaRecord = {
  minimap2: 'Aligning with minimap2',
  bwa: 'Aligning with BWA-MEM',
  samtools: 'Sorting an alignment',
  bcftools: 'Calling what the reads show',
}

export default meta
