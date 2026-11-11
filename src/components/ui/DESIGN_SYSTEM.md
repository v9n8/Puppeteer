# Puppeteer design system

Single reference for how UI is built in `src/` (app + `controller/`). Before
adding UI, reuse what is listed here; if something is missing, extend the shared
primitive (or add a token) instead of hand-writing classes in a screen.

Primitives live in `src/components/ui/` and are imported from `@/components/ui`.

## 1. Tokens

### Colors (`src/assets/main.css`)

The default theme is flat **Zinc + Indigo**, tuned for mid-range IPS monitors:
neutral surfaces, one indigo accent, 1px solid borders, no glass and no
shadows on in-page surfaces (shadows only on popovers, menus, toasts, modals).
Dark mode is charcoal (`#151518` sidebar → `#1f1f23` page → `#2e2e34` card →
`#3c3c44` popover), not deep navy, so the tiers stay apart on a panel with
washed-out blacks.

Use the semantic utilities — they follow light/dark mode and the color themes.

| Purpose | Utility | Never |
|---|---|---|
| Page / surface | `bg-background`, `bg-card`, `bg-sidebar`, `bg-muted` | `bg-white`, `bg-slate-*` |
| Text | `text-foreground`, `text-muted-foreground` | `text-gray-*` |
| Brand / selected | `primary` (`bg-primary`, `text-primary`, `bg-primary/15` for a selected row) | `indigo-*` |
| Hover | `bg-accent` (`hover:bg-accent/50` for rows) | |
| Field / outline-button border | `border-input` (`border-border` is the quiet divider for cards and regions) | |
| Floating layer border (menu, popover, modal, tooltip) | `border-popover-border` | `border-border` |
| Error / delete | `destructive` | `red-*` |
| Success / done / connected | `success` | `emerald-*`, `green-*` |
| Warning / pending / overdue-soon | `warning` | `amber-*`, `yellow-*` |
| Info / neutral highlight | `info` | `sky-*`, `blue-*` |

Tinted surfaces use opacity on the same token: `bg-warning/10 border-warning/20 text-warning`.
Solid fills pair with their foreground: `bg-success text-success-foreground`,
`bg-warning text-warning-foreground`, `bg-info text-info-foreground`,
`bg-destructive text-destructive-foreground` (never `text-white` / `text-black`).
Question / plan prompts and the "ok" segment of usage meters use `info` (fixed hue — `primary` changes per color theme and can collide with warning/destructive).

`dark:` variants follow the app's `.dark` class (`@custom-variant dark` in
`main.css`), not the OS setting.

Exceptions (raw palette is OK): `Badge` `info` tone (violet), charts/Gantt series colors,
calendar event colors, decorations — data-driven palettes, not UI state.

### Radius

`rounded-md` controls and buttons · `rounded` inner chips / segments · `rounded-lg`
cards & floating toolbars · `rounded-full` dots, count pills, avatars.

### Typography scale

`text-2xs` and `text-xs` are redefined in `main.css` (`@theme`) so nothing
renders below 12px — smaller text breaks up on low-density IPS panels.

| Size | Use |
|---|---|
| `text-2xs` (12px) | the floor: badges, count pills, Button `xs` text, meta lines, hints under fields, list secondary text, segmented controls, banners, kbd hints |
| `text-xs` (13px) | default UI text: form fields, Button `sm`, list content |
| `text-sm` (14px) | page titles (`PageHeader`), list row primary titles, menu items, body copy |
| `text-base` (16px) | detail-pane title (`DetailHeader`) |
| `text-lg`+ | empty states / About only |

Never use arbitrary sizes (`text-[11px]`, `text-[13px]`…): pick a step from the
table. Weights: 400 body, 500 labels/badges, 600 titles — no 300.

### Icons (lucide)

| Context | Size | Stroke |
|---|---|---|
| inside Button `xs`, inline meta, inline retry | `h-3 w-3` | 2 (1.75 for outlined glyphs) |
| inside Button `sm` / `icon-sm` / `icon`, menu items, list rows | `h-3.5 w-3.5` | 1.75 |
| inside Button `md` / `icon-lg`, card actions | `h-4 w-4` | 1.75–2 |
| empty states | `h-6 w-6` in a `h-12 w-12 rounded-xl bg-muted` tile | 1.5 |

## 2. Sizing scale — one height per size, shared by every control

| Size | Height | Button | Input / AppSelect / DatePicker | Icon button |
|---|---|---|---|---|
| — | 24px `h-6` | — | — | `size="icon-xs"` (dense spots only) |
| `xs` | 28px `h-7` | `size="xs"` (12px text) | `size="xs"` | `size="icon-sm"` |
| `sm` | 32px `h-8` | `size="sm"` (default) | `size="sm"` (default) | `size="icon"` |
| `md` | 36px `h-9` | `size="md"` | `size="md"` | `size="icon-lg"` |

Heights come from `controlHeight` in `controlStyles.ts`. Controls placed in the
same row must use the same size, so their edges line up. Never override height
with `class="h-…"` on a shared control.

## 3. Buttons — which size/variant where

Always `<Button>`; a raw `<button>` is only for non-button-looking hit areas
(a list row, a tab, a checkbox square, an inline text link, a chip's ×).

| Place | Size | Variant |
|---|---|---|
| `PageHeader` actions | `sm` / `icon` | `outline` (primary action `primary`) |
| List sidebar toolbar (next to search) | `icon` | `outline` (`primary` when toggled on, e.g. pin) |
| List sidebar "+ New" | `sm` | `primary` |
| Detail-pane header (`DetailHeader`, Note/Todo/Secret/Slack headers): Save, Cancel, More ⋯, Edit, Done, plus the project select / due date / Sessions in that row | `sm` / `icon` (32px — same as the list sidebar toolbar beside it) | Save `primary`, others `outline` |
| `DetailSection` actions (Add, Test…) | `xs` | `outline` |
| Inline row actions (remove a row, clear a field) | `icon-sm` | `destructive-ghost` / `ghost` |
| Dense spots (banner retry, chip ×, folder-header actions, h-8 sub-toolbars) | `icon-xs` | `ghost` / `destructive-ghost` |
| Card hover toolbars | `icon-sm` | `ghost` / `destructive-ghost` |
| Modal / Drawer footer | `sm` | Cancel `ghost`, confirm `primary`, delete `destructive` |
| Empty state CTA | `sm` | `primary` (+ secondary `outline`) |

Toggled icon buttons: `:variant="on ? 'primary' : 'outline'"` + `:aria-pressed`.
Every icon-only button needs `:title` and `:aria-label`.

## 4. Layout patterns

- **Full-page view**: `PageHeader` (h-13, px-6) → optional toolbar strip
  (`border-b border-border/60 px-6 py-2`) → scrolling content.
- **List-detail screen** (Skills, Rules, MCP, Servers, Notes/Todos, Slack, Secret
  notes): `listDetail/ListDetailSidebar` (280px) + right pane with
  `listDetail/DetailHeader`, body of `listDetail/DetailSection`s, and
  `listDetail/DetailEmpty` when nothing is selected.
- **Switchers** (filters, view modes, auth method): `SegmentedControl`.
- **Dropdowns**: `AppSelect` for values, `DropdownMenu` + `DropdownItem` for actions.
- **Dialogs**: `Modal`, `ConfirmModal`, `PromptModal`, `Drawer` — never a custom overlay.
- **On/off**: `ToggleSwitch` — `size="sm"` in list rows/cards, `size="md"` in
  settings rows; `:checked="'mixed'"` for partially-enabled groups.
- **Status pill**: `Badge` / `StatusBadge`; count pill: the one in `PageHeader`
  (`#leading` slot for a back button or page icon).
- **Inline messages**: `Banner tone="destructive|warning|success|info"` (+ `#actions`
  slot for retry/dismiss) — never a hand-copied tinted box.

## 5. Remote controller (`src/controller/`, phones)

Same tokens (§1), touch-first layout — don't use `PageHeader` / list-detail or
desktop `xs`/`icon-sm` sizes here.

| Piece | Size | Use |
|---|---|---|
| `ControllerHeader` | h-12, px-3, title `text-base font-semibold` | header of every tab/editor |
| `HeaderButton` | 32px, icon `h-4 w-4` | back / refresh / disconnect, plain icon buttons in the composer |
| `FabButton` | 56px round, bottom-right | "+ new" on list tabs |
| `BottomTabBar` | icon `h-5`, label `text-2xs` | tab navigation |
| `SearchField` | full width | top of list tabs |
| Sheets (`ThemeSheet`, `PushSheet`) | scrim `bg-black/50`, `rounded-t-xl` panel | secondary settings |
| List rows | `px-3 py-2.5`, whole row tappable, `border-b border-border/40` | inbox / runs / notes |

Touch targets: ≥ 32px in dense toolbars, ≥ 36px (`Button size="md"`) for decision
buttons, ≥ 44px for a standalone primary action.

## 6. Themes

Palettes live one per file in `src/assets/themes/<id>.css` (imported at the top of `main.css`) and are registered in `src/lib/colorThemes.ts`. Every file defines a `[data-theme='<id>']:not(.dark)` and a `[data-theme='<id>'].dark` block with the full token set plus `--aurora-1/2`; `src/lib/__tests__/themeContrast.test.mjs` enforces WCAG AA on every pair.

A theme changes **colors only**: never radius, font, material or motion, so every component has one look. All palettes follow one rule: each declares its surfaces, text, primary and four status colors; secondary text, `--input`, `--popover-border`, `--border` and the tinted surfaces are derived from the text color until secondary text is ≥ 4.5:1 on every surface (≥ 7:1 in `contrast`), `--input` / `--popover-border` ≥ 3:1, and `--border` a quiet ~1.4:1. A removed palette id is mapped to its replacement in `normalizeColorTheme`.

`html[data-agent-state]` (`idle` · `running` · `waiting` · `error` · `done`) drives the app-wide ambient light (`src/assets/agent-ambient.css`); it pauses under `html[data-animated-bg='false']` and `prefers-reduced-motion`.

## 7. Checklist for a new screen

1. Layout from §4, primitives from `@/components/ui`.
2. Buttons and controls follow §2–§3; same-row controls share one size.
3. Colors only from §1 tokens; icons per the icon table.
4. i18n keys in both `en` and `vi`; titles/aria-labels on icon buttons.
