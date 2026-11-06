import { reactive } from "vue"
import type { ConnectionItem, UserInfo } from "./types"

export const GUEST_LIMIT = 3

export const appState = reactive({
  user: null as UserInfo | null,
  connections: [] as ConnectionItem[],
  booted: false,
})

export const isGuest = () => appState.user === null
