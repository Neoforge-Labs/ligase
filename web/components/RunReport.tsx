import type { ReactNode } from 'react'
import styles from './RunReport.module.css'

/*
 * A `ligase run` transcript.
 *
 * The run report is the other half of the engine's voice. Its whole value is
 * in one word per line — ran, cached, skipped, noticed — so that word gets
 * its own column and its own colour, and the timings sit right-aligned where
 * they can be compared down the column instead of read as prose.
 *
 * As with Diagnostic, the text is the engine's own, verbatim, and the panel
 * carries the same bar: a `FIG. NN` label on the left and, optionally, a note
 * on the right saying what produced it. A report with neither renders no bar
 * at all, so the figure number is opt-out rather than something every
 * transcript has to be edited to receive.
 */

const STATUSES = ['ran', 'cached', 'skipped', 'noticed', 'failed'] as const
type Status = (typeof STATUSES)[number]

const ROW = new RegExp(`^(${STATUSES.join('|')})\\s+(.*?)(?:\\s{2,}(\\S+))?\\s*$`)
const SUMMARY = /^\d+ steps?:/
const COMMAND = /^\$\s+(.*)$/
const HEADING = /^(running|checked|checking)\b/

type Line =
  | { kind: 'command'; text: string }
  | { kind: 'heading'; text: string }
  | { kind: 'row'; status: Status; subject: string; timing?: string }
  | { kind: 'summary'; text: string }
  | { kind: 'plain'; text: string }

function parse(raw: string): Line[] {
  return raw.split('\n').map((line): Line => {
    const command = COMMAND.exec(line)
    if (command) return { kind: 'command', text: command[1] }
    if (SUMMARY.test(line.trim())) return { kind: 'summary', text: line.trim() }
    if (HEADING.test(line.trim())) return { kind: 'heading', text: line.trim() }
    const row = ROW.exec(line)
    if (row) {
      return {
        kind: 'row',
        status: row[1] as Status,
        subject: row[2],
        ...(row[3] ? { timing: row[3] } : {}),
      }
    }
    return { kind: 'plain', text: line }
  })
}

/** Render backticked spans inside a row's subject as code. */
function subject(text: string) {
  return text.split(/(`[^`]+`)/).map((part, i) =>
    part.startsWith('`') && part.endsWith('`') ? (
      <code key={i} className={styles.name}>
        {part.slice(1, -1)}
      </code>
    ) : (
      <span key={i}>{part}</span>
    ),
  )
}

export type RunReportProps = {
  /** The engine's output, verbatim. */
  children: string
  /** What produced it, shown at the right of the panel's bar. */
  note?: ReactNode
  /**
   * Number the panel as a figure, `FIG. NN`, counted in CSS and shared with
   * `Diagnostic` so the two never number against each other. Defaults to true.
   */
  figure?: boolean
}

export function RunReport({ children, note, figure = true }: RunReportProps) {
  const raw = typeof children === 'string' ? children.replace(/\n+$/, '') : ''
  const lines = parse(raw)

  return (
    <div className={styles.panel} role="group" aria-label="Run report">
      {figure || note ? (
        <div className={styles.bar}>
          {figure ? <span className="lg-figure" aria-hidden="true" /> : null}
          {note ? <span className={styles.note}>{note}</span> : null}
        </div>
      ) : null}
      <div className={styles.body}>
        {lines.map((line, i) => {
          switch (line.kind) {
            case 'command':
              return (
                <div key={i} className={styles.command}>
                  <span aria-hidden="true" className={styles.prompt}>
                    $
                  </span>
                  <span>{line.text}</span>
                </div>
              )
            case 'heading':
              return (
                <div key={i} className={styles.heading}>
                  {line.text}
                </div>
              )
            case 'row':
              return (
                <div key={i} className={styles.row} data-status={line.status}>
                  <span className={styles.status}>{line.status}</span>
                  <span className={styles.subject}>{subject(line.subject)}</span>
                  {line.timing ? <span className={styles.timing}>{line.timing}</span> : null}
                </div>
              )
            case 'summary':
              return (
                <div key={i} className={styles.summary}>
                  {line.text}
                </div>
              )
            default:
              return (
                <div key={i} className={line.text.trim() ? styles.plain : styles.blank}>
                  {line.text || ' '}
                </div>
              )
          }
        })}
      </div>
    </div>
  )
}

export default RunReport
