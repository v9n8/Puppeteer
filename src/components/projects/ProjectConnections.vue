<script setup lang="ts">
// Connections tab of the project detail column: the accounts the project may use
// (one filtered list) and which MCP servers it gets.
import ProjectAccounts from '@/components/projects/ProjectAccounts.vue'
import ProjectMcp from '@/components/projects/ProjectMcp.vue'
import type { ProjectAccountType } from '@/lib/projectSections'
import type { Project } from '@/stores/projects'

defineProps<{ project: Project; section: 'accounts' | 'mcp'; accountType: ProjectAccountType | null }>()
</script>

<template>
  <ProjectAccounts v-if="section === 'accounts'" :project="project" :type="accountType" />
  <ProjectMcp v-else :project="project" />
</template>
