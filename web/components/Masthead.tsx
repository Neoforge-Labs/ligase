import styles from './Masthead.module.css'

/*
 * The journal's masthead: volume, place and year in one letterspaced row, with
 * a hairline beneath, as neoforgelabs.tech opens with
 * `VOL. 01 — A RESEARCH HOLDING / ◉ WESTLANDS, NAIROBI / MMXXV`.
 *
 * It is the muted `.lg-mono` voice, not the orange eyebrow: the brand's own
 * masthead is a quiet row, and the orange belongs to the hero's status line
 * beneath it. Hero draws it only when asked to, which only the landing page
 * does; every other page is a numbered section of the journal and has no
 * cover.
 *
 * `VOL. 02` is a placeholder. The parent site is VOL. 01 and the sequence is
 * a guess; which volume this is belongs to the author. MMXXVI is 2026, the
 * year of v0.1.0.
 *
 * The text is written in capitals because `.lg-mono` uppercases it only for
 * the eye: check-assets reads the HTML, and a check that matched on a CSS
 * transform would be matching on something the markup does not say.
 */
export function Masthead() {
  return (
    <div className={`lg-mono ${styles.masthead}`} data-masthead>
      <span>VOL. 02 — THE ENGINE</span>
      <span className={styles.place}>◉ WESTLANDS, NAIROBI</span>
      <span>MMXXVI</span>
    </div>
  )
}

export default Masthead
