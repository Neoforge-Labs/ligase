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
 * rasterises the same everywhere. This is the small variant, the geometry of
 * public/favicon.svg, because the glyph is drawn at 24px and the display
 * variant's 3.5-unit slabs would land at 0.84px there. The rule is in
 * identity.mdx: the small drawing is for anything below 32px, the display
 * drawing (public/icon.svg) for 32px and up.
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
          <rect x="26" y="10" width="16" height="76" />
          <rect x="18" y="10" width="32" height="6" />
          <rect x="15" y="80" width="38" height="6" />
          <rect x="60" y="44" width="16" height="42" />
          <rect x="52" y="44" width="32" height="6" />
          <rect x="49" y="80" width="38" height="6" />
          <circle cx="68" cy="26" r="9" />
        </g>
        <polygon points="26,58 76,52 76,66 26,72" className={styles.tie} />
      </svg>
      <span className={styles.word}>
        Lig<span className={styles.ase}>ase</span>
      </span>
    </span>
  )
}

export default Mark
