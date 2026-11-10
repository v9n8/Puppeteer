// The accounts behind Settings → Connections, one store per type (lib/connectionTypes
// describes the types). Shared by the Connections screen and SettingsView's nav
// badge, so both count the same accounts of the same enabled types.
import { computed } from 'vue'
import { useGithubAccountsStore } from '@/stores/githubAccounts'
import { useGitlabAccountsStore } from '@/stores/gitlabAccounts'
import { useAwsAccountsStore } from '@/stores/awsAccounts'
import { useAzureAccountsStore } from '@/stores/azureAccounts'
import { useGoogleAccountsStore } from '@/stores/googleAccounts'
import { useFeaturesStore } from '@/stores/features'
import {
  enabledConnectionTypes, countConnections,
  type ConnectionSources, type ConnectionTypeId,
} from '@/lib/connectionTypes'

export function useConnectionSources() {
  const gh = useGithubAccountsStore()
  const gl = useGitlabAccountsStore()
  const aws = useAwsAccountsStore()
  const azure = useAzureAccountsStore()
  const google = useGoogleAccountsStore()
  const features = useFeaturesStore()

  const sources = computed<ConnectionSources>(() => ({
    github: gh.accounts,
    gitlab: gl.accounts,
    aws: aws.accounts,
    azure: azure.accounts,
    google: google.accounts,
  }))
  const fetchers: Record<ConnectionTypeId, () => Promise<void>> = {
    github: gh.fetch,
    gitlab: gl.fetch,
    aws: aws.fetch,
    azure: azure.fetch,
    google: google.fetch,
  }
  const types = computed(() => enabledConnectionTypes(features.tree))
  const total = computed(() => countConnections(types.value, sources.value))

  return { sources, fetchers, types, total }
}
