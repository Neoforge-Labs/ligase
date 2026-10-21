import Link from 'next/link'
import type { ReactNode } from 'react'
import styles from './Hazard.module.css'

/*
 * A link to one of the silent-failure pages.
 *
 * Each card states the bug in the field's own vocabulary — the thing someone
 * would type into a search box at 2am — and shows the two values that
 * disagree, because that pairing *is* the bug. The feature that catches it is
 * not mentioned; people arrive at these pages by symptom, not by feature.
 */

export function HazardGrid({ children }: { children: ReactNode }) {
  return <div className={styles.grid}>{children}</div>
}

export function Hazard({
  href,
  symptom,
  left,
  right,
  children,
}: {
  href: string
  /** What the reader would call the problem. */
  symptom: string
  /** The two values that disagree. */
  left: string
  right: string
  children: ReactNode
}) {
  return (
    <Link href={href} className={styles.card}>
      <span className={styles.pair}>
        <code className={styles.left}>{left}</code>
        <span className={styles.vs} aria-label="against">
          ≠
        </span>
        <code className={styles.right}>{right}</code>
      </span>
      <span className={styles.symptom}>{symptom}</span>
      <span className={styles.body}>{children}</span>
    </Link>
  )
}

export default Hazard
