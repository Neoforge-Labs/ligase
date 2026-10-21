import { useMDXComponents as getThemeComponents } from 'nextra-theme-docs'
import { Diagnostic } from './components/Diagnostic'
import { Note } from './components/Note'
import { RunReport } from './components/RunReport'
import { Stat, Stats } from './components/Stats'
import { Hazard, HazardGrid } from './components/Hazard'
import { Lede } from './components/Lede'
import { Action, Band, Hero } from './components/Hero'
import { BrandCard } from './components/BrandCard'

const themeComponents = getThemeComponents()

export function useMDXComponents(components?: Record<string, unknown>) {
  return {
    ...themeComponents,
    Diagnostic,
    RunReport,
    Note,
    Stat,
    Stats,
    Hazard,
    HazardGrid,
    Lede,
    Hero,
    Action,
    Band,
    BrandCard,
    ...components,
  }
}
