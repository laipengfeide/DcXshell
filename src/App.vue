<script setup lang="ts">
import { onMounted, ref } from "vue"
import AppTitleBar from "./components/AppTitleBar.vue"
import LoginView from "./views/LoginView.vue"
import WorkspaceView from "./views/WorkspaceView.vue"
import * as api from "./api"
import { appState } from "./store"
import type { UserInfo } from "./types"

const stage = ref<"boot" | "login" | "workspace">("boot")
const canBack = ref(false)

onMounted(async () => {
  try {
    appState.user = await api.authStatus()
  } catch {
    appState.user = null
  }
  appState.booted = true
  stage.value = appState.user ? "workspace" : "login"
})

function onLoginSuccess(user: UserInfo) {
  appState.user = user
  canBack.value = false
  stage.value = "workspace"
}

async function enterGuest() {
  appState.user = null
  // 访客进入也登记会话与 IP，便于后续局域网互传与日志归属
  try {
    await api.guestEnter()
  } catch {
    /* 日志/会话登记失败不阻塞进入 */
  }
  canBack.value = false
  stage.value = "workspace"
}

async function requestLogin() {
  if (appState.user === null) {
    try {
      await api.guestLeave()
    } catch {
      /* ignore */
    }
  }
  canBack.value = true
  stage.value = "login"
}

function onLogout() {
  appState.user = null
  canBack.value = false
  stage.value = "login"
}
</script>

<template>
  <div class="app-shell">
    <AppTitleBar />
    <div class="app-body">
      <div v-if="stage === 'boot'" class="boot-screen">
        <span class="boot-mark">&gt;_</span>
        <span class="boot-text">Dc Xshell 正在启动…</span>
      </div>
      <LoginView
        v-else-if="stage === 'login'"
        :can-back="canBack"
        @success="onLoginSuccess"
        @guest="enterGuest"
        @back="stage = 'workspace'"
      />
      <WorkspaceView v-else @login-request="requestLogin" @logout="onLogout" />
    </div>
  </div>
</template>

<style scoped>
.app-shell {
  height: 100%;
  display: flex;
  flex-direction: column;
  background: var(--bg-0);
}

.app-body {
  flex: 1;
  min-height: 0;
  position: relative;
}

.boot-screen {
  height: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 14px;
  color: var(--text-dim);
}

.boot-mark {
  width: 54px;
  height: 54px;
  display: grid;
  place-items: center;
  border-radius: 14px;
  background: linear-gradient(135deg, #10b981, #0ea5e9);
  color: #04120f;
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 20px;
  box-shadow: 0 0 34px rgba(45, 212, 191, 0.35);
  animation: bootPulse 1.4s ease-in-out infinite;
}

.boot-text {
  font-size: 13px;
  letter-spacing: 0.4px;
}

@keyframes bootPulse {
  0%,
  100% {
    transform: scale(1);
    opacity: 1;
  }
  50% {
    transform: scale(1.06);
    opacity: 0.75;
  }
}
</style>
