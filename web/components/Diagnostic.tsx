'use client'

import { useState, type ReactNode } from 'react'
import styles from './Diagnostic.module.css'

/*
 * A Ligase diagnostic, rendered the way a compiler renders one.
 *
 * The text passed in is the engine's real output, verbatim. This component
 * only adds typography: it never rewrites a character. That matters because
 * the pages that use it claim to show what the engine actually says, and the
 * copy button hands back the original string.
 *
 * Anything the parser does not recognise falls through to a plain monospace
 * line, so an unfamiliar diagnostic degrades to a well-set code block rather
 * than to mangled output.
 */

/** `DimensionError in step `call` (sample SRR32217714):` */
const HEADER =
  /^([A-Z][A-Za-z]*(?:Error|Failure|Check))\s+in\s+step\s+`([^`]+)`(?:\s+\(sample\s+([^)]+)\))?\s*:\s*$/

/** A trailing `(from ...)` or `(declared ...)` attribution. */
const SOURCE = /\s*\((from\s[^)]*|declared[^)]*)\)\s*$/

/** `  aln.reference = HXB2 (from BAM @SQ header)` */
const BINDING = /^\s+([A-Za-z_][\w.]*(?:\.[\w.]+)*)\s=\s(.+)$/

/**
 * `  ref.reference lists contigs in the order 2, 1 (from FASTA contigs: ...)`
 * — a fact about a binding that is not an equality.
 */
const ATTRIBUTED = /^\s+([A-Za-z_][\w.]*(?:\.[\w.]+)*)\s+(.+)$/

/** `  The same problem affects 3 other samples: SRR32217715, ...` */
const ALSO = /^\s+(?:The same problem affects|Also affects)\b/

type Line =
  | { kind: 'header'; type: string; step: string; sample?: string }
  | { kind: 'binding'; name: string; value: string; source?: string }
  | { kind: 'attributed'; name: string; rest: string; source?: string }
  | { kind: 'advice'; text: string }
  | { kind: 'also'; text: string }
  | { kind: 'plain'; text: string }

/** Split a trailing `(from ...)` attribution off a value. */
function splitSource(text: string): [string, string | undefined] {
  const m = SOURCE.exec(text)
  if (!m) return [text.trimEnd(), undefined]
  return [text.slice(0, m.index).trimEnd(), m[1]]
}

function parse(raw: string): Line[] {
  const rawLines = raw.split('\n')

  /*
   * Every one of the engine's diagnostics closes with the line that tells you
   * what to do about it — "These must match. Did you pass the wrong
   * reference?", "Add a renaming step". Finding it positionally rather than by
   * matching its wording means the emphasis survives the engine rewording its
   * advice, which a list of known phrases would not.
   */
  let adviceAt = -1
  for (let i = rawLines.length - 1; i >= 0; i--) {
    const line = rawLines[i]
    if (!line.trim()) continue
    if (ALSO.test(line)) continue
    if (HEADER.test(line)) break
    adviceAt = i
    break
  }

  return rawLines.map((line, i): Line => {
    const header = HEADER.exec(line)
    if (header) {
      return {
        kind: 'header',
        type: header[1],
        step: header[2],
        ...(header[3] ? { sample: header[3] } : {}),
      }
    }
    if (ALSO.test(line)) return { kind: 'also', text: line.trim() }
    if (i === adviceAt) return { kind: 'advice', text: line.trim() }

    const binding = BINDING.exec(line)
    if (binding) {
      const [value, source] = splitSource(binding[2])
      return { kind: 'binding', name: binding[1], value, ...(source ? { source } : {}) }
    }

    const attributed = ATTRIBUTED.exec(line)
    if (attributed && SOURCE.test(line)) {
      const [rest, source] = splitSource(attributed[2])
      return {
        kind: 'attributed',
        name: attributed[1],
        rest,
        ...(source ? { source } : {}),
      }
    }

    return { kind: 'plain', text: line }
  })
}

export type DiagnosticProps = {
  /** The engine's output, verbatim. */
  children: string
  /**
   * What produced it, shown in the panel's top-right — e.g. `ligase check`.
   * Say `nothing ran` where the point is that no compute happened.
   */
  note?: ReactNode
}

export function Diagnostic({ children, note }: DiagnosticProps) {
  const raw = typeof children === 'string' ? children.replace(/\n+$/, '') : ''
  const lines = parse(raw)
  const [copied, setCopied] = useState(false)

  async function copy() {
    try {
      await navigator.clipboard.writeText(raw)
      setCopied(true)
      setTimeout(() => setCopied(false), 1600)
    } catch {
      /* Clipboard access can be refused; the text is selectable either way. */
    }
  }

  const header = lines.find((l) => l.kind === 'header')

  return (
    <div className={styles.panel} role="group" aria-label="Engine diagnostic">
      <div className={styles.bar}>
        {header?.kind === 'header' ? (
          <span className={styles.barLeft}>
            <span className={styles.type}>{header.type}</span>
            <span className={styles.in}>in step</span>
            <code className={styles.step}>{header.step}</code>
            {header.sample ? <span className={styles.sample}>{header.sample}</span> : null}
          </span>
        ) : (
          <span className={styles.barLeft}>
            <span className={styles.type}>diagnostic</span>
          </span>
        )}
        <span className={styles.barRight}>
          {note ? <span className={styles.note}>{note}</span> : null}
          <button type="button" className={styles.copy} onClick={copy}>
            {copied ? 'copied' : 'copy'}
          </button>
        </span>
      </div>

      <div className={styles.body}>
        {lines.map((line, i) => {
          switch (line.kind) {
            case 'header':
              // Promoted into the bar above; the raw text keeps it for copying.
              return null
            case 'binding':
              return (
                <div key={i} className={styles.binding}>
                  <span className={styles.name}>{line.name}</span>
                  <span className={styles.eq}>=</span>
                  <span className={styles.value}>{line.value}</span>
                  {line.source ? <span className={styles.source}>{line.source}</span> : null}
                </div>
              )
            case 'attributed':
              return (
                <div key={i} className={styles.binding}>
                  <span className={styles.name}>{line.name}</span>
                  <span className={styles.eq} aria-hidden="true" />
                  <span className={styles.value}>{line.rest}</span>
                  {line.source ? <span className={styles.source}>{line.source}</span> : null}
                </div>
              )
            case 'advice':
              return (
                <div key={i} className={styles.question}>
                  <span aria-hidden="true" className={styles.arrow}>
                    →
                  </span>
                  <span>{line.text}</span>
                </div>
              )
            case 'also':
              return (
                <div key={i} className={styles.also}>
                  {line.text}
                </div>
              )
            default: {
              // Hang the wrap under the first character rather than letting it
              // fall back to the left edge, by turning the engine's leading
              // spaces into padding.
              const indent = line.text.length - line.text.trimStart().length
              return (
                <div
                  key={i}
                  className={line.text.trim() ? styles.plain : styles.blank}
                  style={indent ? { paddingLeft: `${indent}ch` } : undefined}
                >
                  {line.text.trimStart() || '\u00a0'}
                </div>
              )
            }
          }
        })}
      </div>
    </div>
  )
}

export default Diagnostic
