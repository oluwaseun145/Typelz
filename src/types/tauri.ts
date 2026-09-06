/** Shape of the getTauriInfo command response. */
export interface TauriInfo {
  version: string
  platform: string
}

/** State event broadcast from the Rust core. */
export interface AppState {
  status: 'idle' | 'loading' | 'ready'
}
