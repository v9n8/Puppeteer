// The permission defaults a phone's `engine_model_options` frame carries (UR-40),
// kept beside the controller protocol so it parses them in one call.

/** The permission mode a run without a pick takes, globally / per project id
 * that sets one; absent from an older Host. */
export interface PermissionDefaults {
  default_permission_mode?: string
  project_permission_modes?: Record<string, string>
}

/** The fields of `o` that are well formed; a non-string project entry is dropped. */
export function parsePermissionDefaults(o: Record<string, unknown>): PermissionDefaults {
  const projects = o.project_permission_modes
  return {
    ...(typeof o.default_permission_mode === 'string' ? { default_permission_mode: o.default_permission_mode } : {}),
    ...(projects && typeof projects === 'object'
      ? {
          project_permission_modes: Object.fromEntries(
            Object.entries(projects).filter((e): e is [string, string] => typeof e[1] === 'string'),
          ),
        }
      : {}),
  }
}
