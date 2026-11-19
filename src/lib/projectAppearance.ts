import {
  FolderOpen, Code, Globe, Server, Database, Smartphone, Bot,
  Rocket, BookOpen, FlaskConical, Wrench, Package, Shield,
} from 'lucide-vue-next'
import type { Component } from 'vue'

/**
 * Tones a project or flag can be tinted with (UX-08). Design-system colours, never
 * raw CSS, so they follow light / dark themes. Listed in colour-wheel order, the
 * order the 6 × 2 picker shows them. Mirrors `PROJECT_COLORS` in
 * `src-tauri/src/commands/projects.rs`, which refuses anything else.
 */
export const PROJECT_COLORS = [
  'neutral', 'primary', 'info', 'teal', 'success', 'lime',
  'yellow', 'warning', 'orange', 'destructive', 'pink', 'violet',
] as const
export type ProjectColor = (typeof PROJECT_COLORS)[number]

/** Lucide icons a project can show instead of the folder. Mirrors `PROJECT_ICONS` in projects.rs. */
export const PROJECT_ICONS = [
  'code', 'globe', 'server', 'database', 'smartphone', 'bot',
  'rocket', 'book-open', 'flask-conical', 'wrench', 'package', 'shield',
] as const
export type ProjectIcon = (typeof PROJECT_ICONS)[number]

const ICON_COMPONENTS: Record<ProjectIcon, Component> = {
  code: Code,
  globe: Globe,
  server: Server,
  database: Database,
  smartphone: Smartphone,
  bot: Bot,
  rocket: Rocket,
  'book-open': BookOpen,
  'flask-conical': FlaskConical,
  wrench: Wrench,
  package: Package,
  shield: Shield,
}

// Full class strings (not built from the token name) so Tailwind sees them in the source.
const SWATCH: Record<ProjectColor, string> = {
  primary: 'bg-primary',
  info: 'bg-info',
  success: 'bg-success',
  warning: 'bg-warning',
  destructive: 'bg-destructive',
  neutral: 'bg-muted-foreground',
  teal: 'bg-tag-teal',
  lime: 'bg-tag-lime',
  yellow: 'bg-tag-yellow',
  orange: 'bg-tag-orange',
  pink: 'bg-tag-pink',
  violet: 'bg-tag-violet',
}
const TEXT: Record<ProjectColor, string> = {
  primary: 'text-primary',
  info: 'text-info',
  success: 'text-success',
  warning: 'text-warning',
  destructive: 'text-destructive',
  neutral: 'text-muted-foreground',
  teal: 'text-tag-teal',
  lime: 'text-tag-lime',
  yellow: 'text-tag-yellow',
  orange: 'text-tag-orange',
  pink: 'text-tag-pink',
  violet: 'text-tag-violet',
}
const TILE: Record<ProjectColor, string> = {
  primary: 'bg-primary/10 border-primary/15 text-primary',
  info: 'bg-info/10 border-info/20 text-info',
  success: 'bg-success/10 border-success/20 text-success',
  warning: 'bg-warning/10 border-warning/20 text-warning',
  destructive: 'bg-destructive/10 border-destructive/20 text-destructive',
  neutral: 'bg-muted border-border text-muted-foreground',
  teal: 'bg-tag-teal/10 border-tag-teal/20 text-tag-teal',
  lime: 'bg-tag-lime/10 border-tag-lime/20 text-tag-lime',
  yellow: 'bg-tag-yellow/10 border-tag-yellow/20 text-tag-yellow',
  orange: 'bg-tag-orange/10 border-tag-orange/20 text-tag-orange',
  pink: 'bg-tag-pink/10 border-tag-pink/20 text-tag-pink',
  violet: 'bg-tag-violet/10 border-tag-violet/20 text-tag-violet',
}

/** A stored colour, or null when it is unset or unknown (the default look). */
export function projectColor(value: string | null | undefined): ProjectColor | null {
  return (PROJECT_COLORS as readonly string[]).includes(value ?? '') ? (value as ProjectColor) : null
}

/** The icon to draw for a project: its own, or the folder. */
export function projectIcon(value: string | null | undefined): Component {
  return ICON_COMPONENTS[value as ProjectIcon] ?? FolderOpen
}

/** A filled dot of the colour (pickers). */
export function swatchClass(color: ProjectColor): string {
  return SWATCH[color]
}

/** Icon colour for a project; '' keeps the surrounding text colour. */
export function projectTextClass(value: string | null | undefined): string {
  const color = projectColor(value)
  return color ? TEXT[color] : ''
}

/** Background, border and icon colour of a project's tile on its card (primary by default). */
export function projectTileClass(value: string | null | undefined): string {
  return TILE[projectColor(value) ?? 'primary']
}

/** The projects a list shows: hidden ones only while "Show hidden projects" is on. */
export function visibleProjects<T extends { hidden: boolean }>(projects: T[], showHidden: boolean): T[] {
  return showHidden ? projects : projects.filter((p) => !p.hidden)
}

/**
 * A drag from `from` to `to` in the shown list, as indexes into the full list
 * (hidden projects included) that the store reorders. The moved project lands
 * just before or after the one it was dropped on, the same as in the shown list;
 * hidden projects in between keep their place relative to the others.
 */
export function fullListMove(
  all: { id: string }[],
  shown: { id: string }[],
  from: number,
  to: number,
): [number, number] {
  const index = (i: number) => all.findIndex((p) => p.id === shown[i]?.id)
  return [index(from), index(to)]
}
