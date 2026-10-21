import type { MetaRecord } from 'nextra'

/*
 * The navigation, ordered by what a reader is doing rather than by how the
 * project is built.
 *
 *   Start here      install it, and get something running
 *   Silent failures the problems it exists to catch — how most people arrive
 *   Guides          one task at a time
 *   Concepts        why it works the way it does
 *   Reference       things you look up: CLI, errors, dimensions, API
 *   Notebooks       worked examples, executed on every build
 *   Project         written about Ligase rather than for someone using it:
 *                   internals, decisions, milestones, validation, releasing
 *
 * The last section is the point of the split. All of it is worth having and
 * none of it belongs in a user's path.
 */
const meta: MetaRecord = {
  index: {
    title: 'Home',
    theme: { sidebar: false, toc: false, pagination: false, breadcrumb: false, layout: 'full' },
  },
  start: { title: 'Start here' },
  'silent-failures': { title: 'Silent failures' },
  guide: { title: 'Guides' },
  concepts: { title: 'Concepts' },
  reference: { title: 'Reference' },
  notebooks: { title: 'Notebooks' },
  project: { title: 'Project' },
  licence: { title: 'Licence' },
}

export default meta
