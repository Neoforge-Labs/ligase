import type { ReactNode } from 'react'
import styles from './Note.module.css'

/*
 * The callout, in the four kinds the old site used — which map onto the four
 * signal colours rather than onto a generic info/warning pair.
 *
 * Titled by default: every admonition in the ported docs had a title, and
 * they are good titles, so they are set as the first thing you read.
 */

export type NoteKind = 'note' | 'warning' | 'tip' | 'caution'

const LABEL: Record<NoteKind, string> = {
  note: 'Note',
  warning: 'Warning',
  tip: 'Tip',
  caution: 'Careful',
}

export function Note({
  kind = 'note',
  title,
  children,
}: {
  kind?: NoteKind
  title?: ReactNode
  children: ReactNode
}) {
  return (
    <aside className={styles.note} data-kind={kind}>
      <p className={styles.head}>
        <span className={styles.label}>{LABEL[kind]}</span>
        {title ? <span className={styles.title}>{title}</span> : null}
      </p>
      <div className={styles.body}>{children}</div>
    </aside>
  )
}

export default Note
