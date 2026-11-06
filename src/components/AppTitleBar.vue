<script setup lang="ts">
import { getCurrentWindow } from "@tauri-apps/api/window"
import { onMounted, ref } from "vue"

const win = getCurrentWindow()
const maximized = ref(false)

async function syncMaximized() {
  try {
    maximized.value = await win.isMaximized()
  } catch {
    /* ignore */
  }
}

async function minimize() {
  try {
    await win.minimize()
  } catch {
    /* ignore */
  }
}

async function toggleMaximize() {
  try {
    await win.toggleMaximize()
    syncMaximized()
  } catch {
    /* ignore */
  }
}

async function closeWindow() {
  try {
    await win.close()
  } catch {
    /* ignore */
  }
}

onMounted(() => {
  syncMaximized()
  window.addEventListener("resize", syncMaximized)
})
</script>

<template>
  <header class="titlebar">
    <div class="brand">
      <span class="brand-mark">&gt;_</span>
      <span class="brand-name">Dc <b>Xshell</b></span>
    </div>
    <div class="drag" data-tauri-drag-region @dblclick="toggleMaximize"></div>
    <div class="win-controls">
      <button class="wc" title="最小化" @click="minimize">
        <svg width="11" height="11" viewBox="0 0 11 11"><path d="M1 5.5h9" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
      <button class="wc" :title="maximized ? '还原' : '最大化'" @click="toggleMaximize">
        <svg v-if="!maximized" width="11" height="11" viewBox="0 0 11 11">
          <rect x="1.4" y="1.4" width="8.2" height="8.2" fill="none" stroke="currentColor" stroke-width="1.2" />
        </svg>
        <svg v-else width="11" height="11" viewBox="0 0 11 11">
          <rect x="1" y="3" width="7" height="7" fill="none" stroke="currentColor" stroke-width="1.1" />
          <path d="M3.5 1.5h6v6" fill="none" stroke="currentColor" stroke-width="1.1" />
        </svg>
      </button>
      <button class="wc close" title="关闭" @click="closeWindow">
        <svg width="11" height="11" viewBox="0 0 11 11"><path d="M1.6 1.6l7.8 7.8M9.4 1.6L1.6 9.4" stroke="currentColor" stroke-width="1.2" /></svg>
      </button>
    </div>
  </header>
</template>
