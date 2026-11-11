<script setup lang="ts">
// Library tab of the project detail column: which skills, rules and MCP servers
// the project gets, and the skill/rule sync conflicts to resolve. Kept as it was
// on the old settings page until the Library screen takes it over (UX-57).
import { computed, ref, watch } from 'vue'
import { useI18n } from 'vue-i18n'
import { GitMerge, Puzzle, ScrollText, Server } from 'lucide-vue-next'
import { Badge, Banner, Button, Card, ToggleSwitch } from '@/components/ui'
import ProjectGroupToggles from '@/components/ProjectGroupToggles.vue'
import { useToast } from '@/composables/useToast'
import { composerDefaults } from '@/lib/composerChips'
import { useProjectsStore, type AppliedRule, type AppliedSkill, type InstructionWarning, type Project } from '@/stores/projects'
import { useSkillsStore } from '@/stores/skills'
import { useRulesStore } from '@/stores/rules'
import type { GroupKind, ProjectGroupState } from '@/stores/groups'
import { useMcpServersStore, type ProjectBuiltinMcp, type ProjectMcpServer } from '@/stores/mcpServers'
import { useAppSettingsStore } from '@/stores/appSettings'
import { useFeaturesStore } from '@/stores/features'

const props = defineProps<{ project: Project; section: 'skills' | 'rules' | 'mcp' | 'conflicts' }>()

const { t } = useI18n()
const { toast } = useToast()
const projectStore = useProjectsStore()
const skillsStore = useSkillsStore()
const rulesStore = useRulesStore()
const mcpStore = useMcpServersStore()
const appSettings = useAppSettingsStore()
const features = useFeaturesStore()

const projectId = computed(() => props.project.id)

const projectConflicts = computed(() => projectStore.conflicts.filter(c => c.project_id === projectId.value))
const projectRuleConflicts = computed(() => projectStore.ruleConflicts.filter(c => c.project_id === projectId.value))

const appliedSkills = ref<AppliedSkill[]>([])
const loadingSkills = ref(false)

const targetLabel = computed<Record<string, string>>(() => ({
  claude: 'Claude',
  codex: 'Codex',
  both: t('projectDetail.targetBoth'),
}))
const appliedRules = ref<AppliedRule[]>([])
const loadingRules = ref(false)
// How Claude/Codex will load this project's instruction files — warnings only, never auto-fixed.
const instructionWarnings = ref<InstructionWarning[]>([])
async function loadInstructionWarnings() {
  const id = projectId.value
  const warnings = await projectStore.getInstructionWarnings(id).catch(() => [])
  if (id === projectId.value) instructionWarnings.value = warnings
}
function instructionWarningText(w: InstructionWarning): string {
  switch (w.code) {
    case 'agents_md_too_large':
      return t('projectDetail.warnAgentsMdTooLarge', { size: Math.ceil(w.size / 1024), limit: Math.floor(w.limit / 1024) })
    case 'claude_reads_agents_md':
      return t('projectDetail.warnClaudeReadsAgentsMd')
    case 'claude_md_imports_agents_md':
      return t('projectDetail.warnClaudeMdImportsAgentsMd', { path: w.path })
    case 'codex_reads_claude_md':
      return t('projectDetail.warnCodexReadsClaudeMd', { config: w.config })
    case 'links_not_ignored':
      return t('projectDetail.warnLinksNotIgnored', { dir: w.dir })
    case 'legacy_copies_backed_up':
      return t('projectDetail.warnLegacyCopiesBackedUp', { dir: w.dir, count: w.batches })
    case 'tracked_by_git':
      return t('projectDetail.warnTrackedByGit', { paths: w.paths.join(', ') })
  }
}
const togglingRuleId = ref<string | null>(null)
// Unified list of every rule with its per-project applied state, mirroring the
// MCP servers tab (single toggleable list instead of add/applied panels).
// `manual` (this switch) and `from_group` are two independent links — either one on its own keeps
// the rule applied, so the switch only ever controls the direct link.
const ruleItems = computed(() =>
  rulesStore.rules.map(r => {
    const applied = appliedRules.value.find(a => a.rule_id === r.id)
    return {
      ...r,
      applied: !!applied,
      manual: applied?.manual ?? false,
      from_group: applied?.from_group ?? false,
      has_claude: applied?.has_claude ?? false,
      has_codex: applied?.has_codex ?? false,
      codex_skill: applied?.codex_skill ?? false,
    }
  })
)

// --- MCP servers (per-project enable/disable) ---
const mcpServers = ref<ProjectMcpServer[]>([])
const loadingMcp = ref(false)
const savingMcp = ref(false)
const togglingMcpId = ref<string | null>(null)
// Warn (not block) when a legacy SSE server is enabled but the default engine is
// Codex, which supports stdio + streamable HTTP MCP. The project's default first (UX-19).
const defaultIsCodex = computed(
  () => composerDefaults({ settings: appSettings.settings, project: props.project.session_defaults }).engine === 'codex',
)

// Built-in servers (puppeteer, google): on by default, switched off per project.
const builtinMcp = ref<ProjectBuiltinMcp[]>([])
const togglingBuiltin = ref<string | null>(null)

async function loadProjectMcpServers() {
  const id = projectId.value
  loadingMcp.value = true
  try {
    const [servers, builtins] = await Promise.all([
      mcpStore.listForProject(id),
      mcpStore.listBuiltinForProject(id),
    ])
    if (id !== projectId.value) return
    mcpServers.value = servers
    builtinMcp.value = builtins
  } finally {
    loadingMcp.value = false
  }
}

async function handleToggleBuiltinMcp(item: ProjectBuiltinMcp) {
  const next = !item.enabled_for_project
  item.enabled_for_project = next
  togglingBuiltin.value = item.name
  try {
    await mcpStore.setBuiltinForProject(projectId.value, item.name, next)
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    item.enabled_for_project = !next
    toast.error(String(e))
  } finally {
    togglingBuiltin.value = null
  }
}

async function handleToggleMcpServer(server: ProjectMcpServer) {
  const next = !server.enabled_for_project
  server.enabled_for_project = next
  savingMcp.value = true
  togglingMcpId.value = server.id
  try {
    const ids = mcpServers.value.filter(s => s.enabled_for_project).map(s => s.id)
    await mcpStore.setForProject(projectId.value, ids)
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    server.enabled_for_project = !next
    toast.error(String(e))
  } finally {
    savingMcp.value = false
    togglingMcpId.value = null
  }
}

async function loadAppliedRules() {
  const id = projectId.value
  loadingRules.value = true
  try {
    const rules = await projectStore.getAppliedRules(id)
    if (id === projectId.value) appliedRules.value = rules
  } finally {
    loadingRules.value = false
  }
  await loadInstructionWarnings()
}

// Direct link only — see `handleToggleSkill`.
async function handleToggleRule(rule: { id: string; manual: boolean }) {
  togglingRuleId.value = rule.id
  try {
    if (rule.manual) {
      await projectStore.removeRuleFromProject(projectId.value, rule.id)
    } else {
      await projectStore.applyRule(projectId.value, rule.id)
    }
    await Promise.all([loadAppliedRules(), loadGroupStates('rule')])
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    togglingRuleId.value = null
  }
}

// ── Skill / rule groups: flip a whole bundle on or off for this project ──────
const skillGroupStates = ref<ProjectGroupState[]>([])
const ruleGroupStates = ref<ProjectGroupState[]>([])
const togglingGroupId = ref<string | null>(null)

async function loadGroupStates(kind: GroupKind) {
  const id = projectId.value
  // Off, the backend refuses every group command (`library.groups`).
  const states = (await features.isOnAfterLoad('library.groups'))
    ? await projectStore.getProjectGroups(kind, id)
    : []
  if (id !== projectId.value) return
  if (kind === 'skill') skillGroupStates.value = states
  else ruleGroupStates.value = states
}

/** Re-read both the group states and the per-item list they just changed. */
async function refreshAfterGroupChange(kind: GroupKind) {
  await loadGroupStates(kind)
  if (kind === 'skill') await loadAppliedSkills()
  else await loadAppliedRules()
}

async function handleToggleGroup(kind: GroupKind, group: ProjectGroupState) {
  if (group.member_count === 0) return
  togglingGroupId.value = group.group_id
  try {
    // The group link is independent of the per-item switches: this toggle only adds or drops that
    // link, so it reads straight off `linked`.
    if (group.linked) {
      const outcome = await projectStore.disableGroup(kind, projectId.value, group.group_id)
      const kept = outcome.kept_manual + outcome.kept_other_group
      toast.success(
        kept > 0
          ? t('groups.toastGroupDisabledKept', { name: group.name, removed: outcome.removed, kept })
          : t('groups.toastGroupDisabled', { name: group.name, removed: outcome.removed }),
      )
    } else {
      const outcome = await projectStore.enableGroup(kind, projectId.value, group.group_id)
      if (outcome.failures.length > 0) {
        const details = outcome.failures.map(f => `• ${f.item_name}: ${f.error}`).join('\n')
        toast.error(t('groups.toastGroupEnabledPartial', { name: group.name, applied: outcome.applied, details }))
      } else {
        toast.success(t('groups.toastGroupEnabled', { name: group.name, count: outcome.applied }))
      }
    }
    await refreshAfterGroupChange(kind)
  } catch (e) {
    toast.error(String(e))
  } finally {
    togglingGroupId.value = null
  }
}

/** A linked group whose members were partly switched off by hand: re-apply the missing ones. */
async function handleReapplyGroup(kind: GroupKind, group: ProjectGroupState) {
  togglingGroupId.value = group.group_id
  try {
    const outcome = await projectStore.enableGroup(kind, projectId.value, group.group_id)
    if (outcome.failures.length > 0) {
      const details = outcome.failures.map(f => `• ${f.item_name}: ${f.error}`).join('\n')
      toast.error(t('groups.toastGroupEnabledPartial', { name: group.name, applied: outcome.applied, details }))
    } else {
      toast.success(t('groups.toastGroupEnabled', { name: group.name, count: outcome.applied }))
    }
    await refreshAfterGroupChange(kind)
  } catch (e) {
    toast.error(String(e))
  } finally {
    togglingGroupId.value = null
  }
}

const togglingSkillId = ref<string | null>(null)
// Unified list of every skill with its per-project applied state (see ruleItems).
const skillItems = computed(() =>
  skillsStore.skills.map(s => {
    const applied = appliedSkills.value.find(a => a.skill_id === s.id)
    return {
      ...s,
      applied: !!applied,
      manual: applied?.manual ?? false,
      from_group: applied?.from_group ?? false,
      has_claude: applied?.has_claude ?? false,
      has_codex: applied?.has_codex ?? false,
    }
  })
)

async function loadAppliedSkills() {
  const id = projectId.value
  loadingSkills.value = true
  try {
    const skills = await projectStore.getAppliedSkills(id)
    if (id === projectId.value) appliedSkills.value = skills
  } finally {
    loadingSkills.value = false
  }
  await loadInstructionWarnings()
}

// The switch owns the direct link only: turning it off on a skill a group also provides leaves the
// skill in place (the group still wants it), it just stops being hand-picked.
async function handleToggleSkill(skill: { id: string; manual: boolean }) {
  togglingSkillId.value = skill.id
  try {
    if (skill.manual) {
      await projectStore.removeSkillFromProject(projectId.value, skill.id)
    } else {
      await projectStore.applySkill(projectId.value, skill.id)
    }
    // One skill can complete or break a group, so the group rows have to follow — in parallel, so
    // the row isn't frozen for two round trips.
    await Promise.all([loadAppliedSkills(), loadGroupStates('skill')])
    toast.success(t('projectDetail.toastSaved'))
  } catch (e) {
    toast.error(String(e))
  } finally {
    togglingSkillId.value = null
  }
}

// The whole tab loads once per project, so moving between its sections reads nothing again.
// Each load drops an answer for a project the user already left.
watch(projectId, async () => {
  if (skillsStore.skills.length === 0) await skillsStore.fetchSkills()
  if (rulesStore.rules.length === 0) await rulesStore.fetchRules()
  await appSettings.ensureLoaded()
  await loadAppliedSkills()
  await loadAppliedRules()
  await loadGroupStates('skill')
  await loadGroupStates('rule')
  await loadProjectMcpServers()
}, { immediate: true })
</script>

<template>
  <!-- Skills -->
  <div v-if="section === 'skills'" class="max-w-lg space-y-5">
    <ProjectGroupToggles
      v-if="features.isOn('library.groups')"
      kind="skill"
      :groups="skillGroupStates"
      :toggling="togglingGroupId"
      @toggle="handleToggleGroup('skill', $event)"
      @reapply="handleReapplyGroup('skill', $event)"
    />
    <Banner v-if="instructionWarnings.length > 0" tone="warning">
      <p class="font-medium text-warning">{{ t('projectDetail.instructionWarningsTitle') }}</p>
      <p
        v-for="(w, i) in instructionWarnings"
        :key="i"
        class="mt-1 leading-relaxed"
      >{{ instructionWarningText(w) }}</p>
    </Banner>
    <Card>
      <template #header>
        <Puzzle class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.skills') }}</span>
        <span
          v-if="appliedSkills.length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ appliedSkills.length }}</span>
        <RouterLink to="/skills" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <!-- Loading -->
      <div v-if="loadingSkills" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
          <div class="h-6 w-11 rounded-full bg-muted animate-pulse shrink-0" />
        </div>
      </div>

      <!-- Empty -->
      <div
        v-else-if="skillItems.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.noSkillsPrefix') }}
        <RouterLink to="/skills" class="text-primary hover:underline">{{ t('projectDetail.createOne') }}</RouterLink>
        {{ t('projectDetail.toEnableHere') }}
      </div>

      <!-- Skill list with on/off toggles -->
      <div v-else class="divide-y divide-border/50">
        <div
          v-for="skill in skillItems"
          :key="skill.id"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ skill.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">
                {{ targetLabel[skill.target] }}
              </Badge>
              <Badge v-if="skill.from_group" tone="info" size="xs" class="shrink-0">{{ t('groups.viaGroup') }}</Badge>
              <Badge v-if="skill.has_claude" tone="primary" size="xs" class="shrink-0">.claude/skills</Badge>
              <Badge v-if="skill.has_codex" tone="neutral" size="xs" class="shrink-0">.agents/skills</Badge>
            </div>
            <p v-if="skill.description" class="text-2xs text-muted-foreground truncate mt-0.5">{{ skill.description }}</p>
          </div>
          <!-- Controls the direct link; a skill a group provides stays applied either way. -->
          <ToggleSwitch
            :checked="skill.manual ? true : skill.from_group ? 'mixed' : false"
            :disabled="togglingSkillId === skill.id"
            :loading="togglingSkillId === skill.id"
            :label="skill.manual
              ? t('projectDetail.disableForProject')
              : skill.from_group ? t('groups.viaGroupHint') : t('projectDetail.enableForProject')"
            @toggle="handleToggleSkill(skill)"
          />
        </div>
      </div>
    </Card>
  </div>

  <!-- Rules -->
  <div v-else-if="section === 'rules'" class="max-w-lg space-y-5">
    <ProjectGroupToggles
      v-if="features.isOn('library.groups')"
      kind="rule"
      :groups="ruleGroupStates"
      :toggling="togglingGroupId"
      @toggle="handleToggleGroup('rule', $event)"
      @reapply="handleReapplyGroup('rule', $event)"
    />
    <Banner v-if="instructionWarnings.length > 0" tone="warning">
      <p class="font-medium text-warning">{{ t('projectDetail.instructionWarningsTitle') }}</p>
      <p
        v-for="(w, i) in instructionWarnings"
        :key="i"
        class="mt-1 leading-relaxed"
      >{{ instructionWarningText(w) }}</p>
    </Banner>
    <Card>
      <template #header>
        <ScrollText class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.rules') }}</span>
        <span
          v-if="appliedRules.length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ appliedRules.length }}</span>
        <RouterLink to="/rules" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <!-- Loading -->
      <div v-if="loadingRules" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
          <div class="h-6 w-11 rounded-full bg-muted animate-pulse shrink-0" />
        </div>
      </div>

      <!-- Empty -->
      <div
        v-else-if="ruleItems.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.noRulesPrefix') }}
        <RouterLink to="/rules" class="text-primary hover:underline">{{ t('projectDetail.createOne') }}</RouterLink>
        {{ t('projectDetail.toEnableHere') }}
      </div>

      <!-- Rule list with on/off toggles -->
      <div v-else class="divide-y divide-border/50">
        <div
          v-for="rule in ruleItems"
          :key="rule.id"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ rule.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">
                {{ targetLabel[rule.target] }}
              </Badge>
              <Badge v-if="rule.from_group" tone="info" size="xs" class="shrink-0">{{ t('groups.viaGroup') }}</Badge>
              <Badge v-if="rule.has_claude" tone="primary" size="xs" class="shrink-0">.claude/rules</Badge>
              <Badge v-if="rule.has_codex" tone="neutral" size="xs" class="shrink-0">{{ rule.codex_skill ? '.agents/skills' : 'AGENTS.md' }}</Badge>
            </div>
            <p v-if="rule.description" class="text-2xs text-muted-foreground truncate mt-0.5">{{ rule.description }}</p>
          </div>
          <!-- Controls the direct link; a rule a group provides stays applied either way. -->
          <ToggleSwitch
            :checked="rule.manual ? true : rule.from_group ? 'mixed' : false"
            :disabled="togglingRuleId === rule.id"
            :loading="togglingRuleId === rule.id"
            :label="rule.manual
              ? t('projectDetail.disableForProject')
              : rule.from_group ? t('groups.viaGroupHint') : t('projectDetail.enableForProject')"
            @toggle="handleToggleRule(rule)"
          />
        </div>
      </div>
    </Card>
  </div>

  <!-- MCP servers -->
  <div v-else-if="section === 'mcp'" class="max-w-lg space-y-5">
    <Card>
      <template #header>
        <Server class="h-3.5 w-3.5 text-muted-foreground" :stroke-width="1.5" />
        <span class="text-xs font-semibold">{{ t('projectDetail.mcpServers') }}</span>
        <span
          v-if="mcpServers.filter(s => s.enabled_for_project).length > 0"
          class="flex h-4 min-w-4 items-center justify-center rounded-full bg-muted px-1 text-2xs font-medium text-muted-foreground leading-none"
        >{{ mcpServers.filter(s => s.enabled_for_project).length }}</span>
        <RouterLink to="/mcp" class="ml-auto text-2xs text-primary hover:underline">{{ t('projectDetail.manage') }}</RouterLink>
      </template>

      <!-- Codex warning banner -->
      <Banner
        v-if="defaultIsCodex && mcpServers.some(s => s.transport === 'sse' && s.enabled_for_project)"
        tone="warning"
        class="mx-4 mt-4 leading-relaxed"
      >
        {{ t('projectDetail.codexWarning') }}
      </Banner>

      <!-- Built-in servers (on by default; off per project) -->
      <div v-if="!loadingMcp && builtinMcp.length" class="divide-y divide-border/50 border-b border-border/50">
        <div
          v-for="item in builtinMcp"
          :key="item.name"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ item.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">{{ t('projectDetail.builtinMcp') }}</Badge>
              <Badge
                v-if="!item.enabled_globally"
                tone="neutral"
                size="xs"
                class="shrink-0"
                :title="t('projectDetail.builtinMcpOffGloballyTitle')"
              >{{ t('projectDetail.disabled') }}</Badge>
            </div>
            <p class="text-2xs text-muted-foreground truncate mt-0.5">mcp__{{ item.name }}__*</p>
          </div>
          <ToggleSwitch
            :checked="item.enabled_for_project"
            :disabled="togglingBuiltin !== null"
            :loading="togglingBuiltin === item.name"
            :label="item.enabled_for_project ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
            @toggle="handleToggleBuiltinMcp(item)"
          />
        </div>
      </div>

      <!-- Loading -->
      <div v-if="loadingMcp" class="divide-y divide-border/50">
        <div v-for="i in 2" :key="i" class="flex items-center gap-3 px-4 py-3">
          <div class="flex-1 space-y-1.5">
            <div class="h-2.5 w-24 bg-muted animate-pulse rounded" />
            <div class="h-2 w-36 bg-muted animate-pulse rounded" />
          </div>
          <div class="h-6 w-11 rounded-full bg-muted animate-pulse shrink-0" />
        </div>
      </div>

      <!-- Empty -->
      <div
        v-else-if="mcpServers.length === 0"
        class="px-4 py-6 text-center text-xs text-muted-foreground"
      >
        {{ t('projectDetail.noMcpPrefix') }}
        <RouterLink to="/mcp" class="text-primary hover:underline">{{ t('projectDetail.createOne') }}</RouterLink>
        {{ t('projectDetail.toEnableHere') }}
      </div>

      <!-- Server list with on/off toggles -->
      <div v-else class="divide-y divide-border/50">
        <div
          v-for="server in mcpServers"
          :key="server.id"
          class="flex items-center gap-3 px-4 py-3"
        >
          <div class="flex-1 min-w-0">
            <div class="flex items-center gap-1.5 flex-wrap">
              <p class="text-xs font-medium font-mono truncate">{{ server.name }}</p>
              <Badge tone="neutral" size="xs" class="shrink-0 uppercase tracking-wide">{{ server.transport }}</Badge>
              <Badge v-if="!server.enabled" tone="neutral" size="xs" class="shrink-0">{{ t('projectDetail.disabled') }}</Badge>
              <Badge
                v-if="server.transport === 'sse' && defaultIsCodex"
                tone="warning"
                size="xs"
                class="shrink-0"
                :title="t('projectDetail.claudeOnlyTitle')"
              >{{ t('projectDetail.claudeOnly') }}</Badge>
            </div>
            <p v-if="server.description" class="text-2xs text-muted-foreground truncate mt-0.5">{{ server.description }}</p>
          </div>
          <ToggleSwitch
            :checked="server.enabled_for_project"
            :disabled="savingMcp"
            :loading="togglingMcpId === server.id"
            :label="server.enabled_for_project ? t('projectDetail.disableForProject') : t('projectDetail.enableForProject')"
            @toggle="handleToggleMcpServer(server)"
          />
        </div>
      </div>
    </Card>
  </div>

  <!-- Conflicts -->
  <div v-else class="max-w-lg space-y-3">
    <!-- Skill conflicts -->
    <Card
      v-for="conflict in projectConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.skill_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictSkillBadge', { path: conflict.engine === 'codex' ? '.codex/skills' : '.claude/skills' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveConflict(conflict.id, true); projectStore.fetchConflicts()">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveConflict(conflict.id, false); projectStore.fetchConflicts()">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>

    <!-- Rule conflicts -->
    <Card
      v-for="conflict in projectRuleConflicts"
      :key="conflict.id"
      class="border-warning/20"
      bodyClass="p-4"
    >
      <div class="flex items-start gap-3 mb-3">
        <GitMerge class="h-4 w-4 text-warning mt-0.5 shrink-0" :stroke-width="1.75" />
        <div>
          <div class="flex items-center gap-1.5">
            <p class="text-sm font-mono font-medium">{{ conflict.rule_name }}</p>
            <Badge tone="neutral" size="xs" class="uppercase tracking-wide">
              {{ t('projectDetail.conflictRuleBadge', { path: conflict.engine === 'claude' ? '.claude/rules' : 'AGENTS.md' }) }}
            </Badge>
          </div>
          <p class="text-xs text-muted-foreground mt-0.5">
            {{ t('projectDetail.localChangesDetected', { date: new Date(conflict.detected_at).toLocaleDateString() }) }}
          </p>
        </div>
      </div>
      <div class="flex gap-2">
        <Button class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, true)">
          {{ t('projectDetail.overwriteWithCentral') }}
        </Button>
        <Button variant="outline" class="flex-1" @click="projectStore.resolveRuleConflict(conflict.id, false)">
          {{ t('projectDetail.keepLocal') }}
        </Button>
      </div>
    </Card>
  </div>
</template>
