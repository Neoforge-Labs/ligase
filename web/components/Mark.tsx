import styles from './Mark.module.css'

/*
 * The wordmark.
 *
 * A ligase joins two strands. The mark is two offset bars — a nick — closed
 * by a single vertical stroke in the brand colour. It is the smallest honest
 * picture of what the tool does, and it survives being 20px tall in a navbar,
 * which a double helix does not.
 */
export function Mark() {
  return (
    <span className={styles.wrap}>
      <svg
        className={styles.glyph}
        width="26"
        height="18"
        viewBox="0 0 26 18"
        fill="none"
        aria-hidden="true"
      >
        <rect x="0" y="3" width="11" height="2.4" rx="1.2" fill="currentColor" />
        <rect x="15" y="3" width="11" height="2.4" rx="1.2" fill="currentColor" />
        <rect x="0" y="12.6" width="11" height="2.4" rx="1.2" fill="currentColor" />
        <rect x="15" y="12.6" width="11" height="2.4" rx="1.2" fill="currentColor" />
        <rect
          x="12.1"
          y="1"
          width="2.8"
          height="16"
          rx="1.4"
          className={styles.join}
        />
      </svg>
      <span className={styles.word}>Ligase</span>
    </span>
  )
}

export default Mark
