import Link from 'next/link'
import type { CSSProperties, ReactNode } from 'react'
import styles from './BrandCard.module.css'
import { Mark } from './Mark'

/*
 * The sub-brand card lockup: how a Neoforge Labs product is presented beside
 * its siblings. Neoforge's own cards carry photography; this one deliberately
 * does not, because inventing an image would be a claim about a product that
 * does not exist. What remains is the structure: a code row set in the mono
 * voice, a rule in the product's own colour, the name in the display face and
 * a single line saying what it is.
 *
 * The title defaults to the Ligase wordmark and the tagline to the one-line
 * description of the project, so the identity page can render the lockup with
 * just a code and a colour. "Open" is only drawn when there is somewhere to
 * open: a card that offers a destination it does not have is a dead control.
 */

const TAGLINE = 'Biological data that carries its dimensions, checked before anything runs.'

export function BrandCard({
  code,
  title = <Mark />,
  tagline = TAGLINE,
  colour,
  href,
}: {
  code: string
  title?: ReactNode
  tagline?: ReactNode
  colour: string
  href?: string
}) {
  const style = { '--card-colour': colour } as CSSProperties
  const body = (
    <>
      <div className={`lg-mono ${styles.row}`}>
        <span>{code}</span>
        {href ? <span aria-hidden="true">→ Open</span> : null}
      </div>
      <div className={styles.foot}>
        <div className={styles.hairline} aria-hidden="true" />
        <div className={styles.title}>{title}</div>
        <div className={styles.tagline}>{tagline}</div>
      </div>
    </>
  )

  if (!href) {
    return (
      <div className={styles.card} style={style}>
        {body}
      </div>
    )
  }
  const external = href.startsWith('http')
  return external ? (
    <a className={styles.card} style={style} href={href} rel="noreferrer">
      {body}
    </a>
  ) : (
    <Link className={styles.card} style={style} href={href}>
      {body}
    </Link>
  )
}

export default BrandCard
