import { execFileSync } from 'node:child_process'
import { join } from 'node:path'

/*
 * When each page last changed, read from git.
 *
 * `lastmod` in a sitemap is a crawl-scheduling hint: a crawler that trusts it
 * revisits what moved and leaves the rest alone. Google says outright that it
 * ignores the value when it looks unreliable, and the usual way to make it
 * unreliable is to emit the build time — then every page claims to have
 * changed on every deploy, which is false for the 109 that did not. So the
 * date here is the commit date of the page's own source file, and a page whose
 * source is not in git gets no `lastmod` at all rather than an invented one.
 *
 * One `git log` invocation, not one per page. The log is walked newest-first
 * and the first date seen for a path is that path's last change; later (older)
 * commits touching it are ignored. 110 pages would otherwise be 110 processes.
 *
 * Pages with no date, deliberately:
 *   reference/api/*, reference/cli, notebooks/*  generated at build time and
 *   gitignored, so they have no history of their own. Their content follows
 *   the package's docstrings and the engine's behaviour, not a file in web/,
 *   and dating them by the generator's own commit would say they changed when
 *   they did not. Some of them were committed once, before they were ignored,
 *   so the log still carries them — see trackedPaths for why that is not
 *   enough to date a page.
 *
 * If git is unavailable — a tarball, a shallow clone with no history for a
 * path — every page simply has no date. The sitemap is still valid; it says
 * less. `.github/workflows/web.yml` checks out with `fetch-depth: 0`, so the
 * deployed sitemap is the complete version.
 */

const CONTENT = 'content'

/** route -> ISO 8601 commit date of the MDX file behind it. */
export type Lastmod = ReadonlyMap<string, Date>

/** `content/concepts/caching.mdx` for `/concepts/caching/`, and index.mdx for a section. */
function sourceOf(route: string): string {
  const trimmed = route.replace(/^\/|\/$/g, '')
  return trimmed === ''
    ? join(CONTENT, 'index.mdx')
    : join(CONTENT, `${trimmed}.mdx`)
}

function sectionSourceOf(route: string): string {
  const trimmed = route.replace(/^\/|\/$/g, '')
  return join(CONTENT, trimmed, 'index.mdx')
}

/**
 * Every path under content/ with a commit date, newest commit first.
 *
 * `--no-renames` keeps a renamed page's date at the rename rather than
 * following it back to its old name, which is the honest answer for a page
 * that moved: its address changed, so a crawler should look again.
 */
/**
 * The content paths git still tracks, relative to cwd.
 *
 * The log alone is not enough. `git log --name-only` reports a path as it was
 * at the time of each commit, so a file that was once committed and has since
 * been deleted still has a newest entry — and that entry is usually the commit
 * that removed it. The generated API and CLI pages were committed once, before
 * they were gitignored, and dating them by that commit gave ten pages a
 * lastmod of the day their old copies were deleted: stale, and exactly the
 * kind of value this module exists not to send. A path is only dated if it is
 * still in the index.
 */
function trackedPaths(cwd: string): Set<string> {
  try {
    return new Set(
      execFileSync('git', ['ls-files', '--', CONTENT], {
        cwd,
        encoding: 'utf8',
        maxBuffer: 16 * 1024 * 1024,
        stdio: ['ignore', 'pipe', 'ignore'],
      })
        .split('\n')
        .filter((line) => line !== ''),
    )
  } catch {
    return new Set()
  }
}

function gitDates(cwd: string): Map<string, Date> {
  const dates = new Map<string, Date>()
  let log: string
  try {
    log = execFileSync(
      'git',
      // `--relative` because git prints paths from the repository root and
      // this runs in web/: without it every path comes back as
      // `web/content/...` and matches nothing, which is a silent empty map
      // rather than an error. check-seo.mjs fails if no date survives.
      ['log', '--relative', '--no-renames', '--name-only', '--format=%x00%cI', '--', CONTENT],
      { cwd, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024, stdio: ['ignore', 'pipe', 'ignore'] },
    )
  } catch {
    return dates // not a git checkout, or git is not installed
  }
  let when: Date | undefined
  for (const line of log.split('\n')) {
    if (line.startsWith('\0')) {
      const parsed = new Date(line.slice(1).trim())
      when = Number.isNaN(parsed.getTime()) ? undefined : parsed
    } else if (line !== '' && when && !dates.has(line)) {
      dates.set(line, when)
    }
  }
  return dates
}

/** The dates for a set of routes, leaving out any route with no source in git. */
export function lastmodFor(routes: readonly string[], cwd: string = process.cwd()): Lastmod {
  const dates = gitDates(cwd)
  const tracked = trackedPaths(cwd)
  const out = new Map<string, Date>()
  if (dates.size === 0 || tracked.size === 0) return out
  const dateOf = (path: string) => (tracked.has(path) ? dates.get(path) : undefined)
  for (const route of routes) {
    const when = dateOf(sourceOf(route)) ?? dateOf(sectionSourceOf(route))
    if (when) out.set(route, when)
  }
  return out
}
