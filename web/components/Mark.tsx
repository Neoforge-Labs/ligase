import styles from './Mark.module.css'

/*
 * The mark and the wordmark.
 *
 * Ligase and ligature share a Latin root, ligare, to bind. A ligature is the
 * typographer's word for two letters cast as one piece of type, and that is
 * what this draws: the l and the i of "Ligase" joined at the waist by a single
 * tie. A tall serif stem, a shorter stem with its tittle, and one slanted bar
 * across both. It is the smallest honest picture of what the tool does, which
 * is to refuse to let two things be connected unless they belong together.
 *
 * The tie is a slanted polygon, not a flat rectangle. Neoforge italicises one
 * meaningful word, Labs, and sets it in orange; the slant is how this mark
 * speaks the same language. Do not flatten it.
 *
 * The letterforms take currentColor and the tie takes the brand token, so the
 * single-colour variant needs no second drawing. Nothing here depends on a
 * webfont: stems, serifs and tittle are rectangles and a circle, so the glyph
 * rasterises the same everywhere. This is the display variant, the geometry of
 * public/icon.svg. At 16px it collapses, which is why public/favicon.svg is a
 * separate drawing with thicker slabs and a flatter tie.
 *
 * The wordmark is Lig plus an italic orange ase. -ase is the suffix most
 * enzyme names carry by IUBMB convention (not all: trypsin, pepsin and papain
 * predate it), so it parallels Neoforge's own italic Labs.
 */
export function Mark() {
  return (
    <span className={styles.wrap}>
      <svg
        className={styles.glyph}
        width="24"
        height="24"
        viewBox="0 0 100 100"
        fill="none"
        aria-hidden="true"
      >
        <g fill="currentColor">
          <rect x="30" y="14" width="10" height="68" />
          <rect x="24" y="14" width="22" height="3.5" />
          <rect x="22" y="78.5" width="26" height="3.5" />
          <rect x="62" y="42" width="10" height="40" />
          <rect x="56" y="42" width="22" height="3.5" />
          <rect x="54" y="78.5" width="26" height="3.5" />
          <circle cx="67" cy="28" r="6" />
        </g>
        <polygon points="30,56 72,48 72,57 30,65" className={styles.tie} />
      </svg>
      <span className={styles.word}>
        Lig<span className={styles.ase}>ase</span>
      </span>
    </span>
  )
}

export default Mark
