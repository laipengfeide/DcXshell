<script setup lang="ts">
import { computed, nextTick, onBeforeUnmount, onMounted, reactive, ref, watch } from "vue"
import { Terminal } from "@xterm/xterm"
import { FitAddon } from "@xterm/addon-fit"
import { WebglAddon } from "@xterm/addon-webgl"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { readText, writeText } from "@tauri-apps/plugin-clipboard-manager"
import { ElMessage } from "element-plus"
import * as api from "@/api"
import FileTransferDialog from "./FileTransferDialog.vue"
import type {
  SessionClosedEvent,
  SessionOutputEvent,
  SessionStatus,
} from "@/types"

const props = defineProps<{
  sessionKey: string
  connectionId: number
  name: string
  host: string
  port: number
  username: string
  active: boolean
}>()

const emit = defineEmits<{
  status: [key: string, status: SessionStatus, latency: number]
  close: [key: string]
}>()

const holder = ref<HTMLElement | null>(null)
const status = ref<SessionStatus>("connecting")
const latency = ref(0)

let term: Terminal | null = null
let fitAddon: FitAddon | null = null
let webglAddon: WebglAddon | null = null
let unOutput: UnlistenFn | null = null
let unClosed: UnlistenFn | null = null
let heartbeatTimer: number | null = null
let resizeObserver: ResizeObserver | null = null
let resizeTimer: number | null = null
let disposed = false
let reconnecting = false
let currentSessionId = ""

const ctx = reactive({ visible: false, x: 0, y: 0, hasSelection: false })
const showFiles = ref(false)

const statusLabel = computed(
  () =>
    ({
      connecting: "连接中",
      online: "在线",
      reconnecting: "重连中",
      offline: "已掉线",
    })[status.value]
)

const sleep = (ms: number) => new Promise((resolve) => window.setTimeout(resolve, ms))

function initTerminal() {
  if (!holder.value) return
  term = new Terminal({
    fontFamily: '"Cascadia Mono","Cascadia Code",Consolas,"Courier New",monospace',
    fontSize: 13,
    lineHeight: 1.18,
    letterSpacing: 0,
    cursorBlink: true,
    cursorStyle: "bar",
    scrollback: 12000,
    allowProposedApi: true,
    rightClickSelectsWord: false,
    theme: {
      background: "#080d11",
      foreground: "#d7e4ec",
      cursor: "#2dd4bf",
      cursorAccent: "#080d11",
      selectionBackground: "rgba(45, 212, 191, 0.30)",
      black: "#0b1117",
      red: "#f07178",
      green: "#7ee787",
      yellow: "#ffd479",
      blue: "#7cb7ff",
      magenta: "#d2a6ff",
      cyan: "#57d9d0",
      white: "#e6edf3",
      brightBlack: "#5b6f7d",
      brightRed: "#ff9aa2",
      brightGreen: "#a5f3b4",
      brightYellow: "#ffe7a3",
      brightBlue: "#a5c8ff",
      brightMagenta: "#e7ccff",
      brightCyan: "#8cefe8",
      brightWhite: "#ffffff",
    },
  })

  fitAddon = new FitAddon()
  term.loadAddon(fitAddon)
  term.open(holder.value)

  try {
    const addon = new WebglAddon()
    addon.onContextLoss(() => addon.dispose())
    term!.loadAddon(addon)
    webglAddon = addon
  } catch {
    /* 无法启用 WebGL 时自动回退 Canvas */
  }

  term.onData((data) => {
    api.writeSession(props.sessionKey, data).catch(() => {
      /* 掉线时由心跳统一处理 */
    })
  })

  term.onResize(({ cols, rows }) => {
    api.resizeSession(props.sessionKey, cols, rows).catch(() => {
      /* ignore */
    })
  })

  term.attachCustomKeyEventHandler(handleKey)
  setTimeout(fit, 30)
}

function fit() {
  if (!holder.value || holder.value.clientWidth < 20 || holder.value.clientHeight < 20) return
  try {
    fitAddon?.fit()
  } catch {
    /* ignore */
  }
}

function handleKey(ev: KeyboardEvent) {
  if (ev.type !== "keydown") return true
  const key = ev.key.toLowerCase()

  if (ev.ctrlKey && key === "v") {
    ev.preventDefault()
    ev.stopPropagation()
    void pasteClipboard()
    return false
  }

  if (ev.ctrlKey && key === "c") {
    if (term?.hasSelection()) {
      ev.preventDefault()
      ev.stopPropagation()
      void copySelection()
      return false
    }
    if (ev.shiftKey) {
      ev.preventDefault()
      return false
    }
    // 无选中内容时保留 ^C 发送给远端
    return true
  }

  return true
}

async function copySelection(): Promise<boolean> {
  const text = term?.getSelection() ?? ""
  if (!text) return false
  try {
    await writeText(text)
    return true
  } catch {
    try {
      await navigator.clipboard.writeText(text)
      return true
    } catch {
      ElMessage.error("复制失败：无法访问剪贴板")
      return false
    }
  }
}

async function pasteClipboard() {
  let text = ""
  try {
    text = await readText()
  } catch {
    try {
      text = await navigator.clipboard.readText()
    } catch {
      text = ""
    }
  }
  if (text) term?.paste(text)
  term?.focus()
}

function clearScreen() {
  term?.clear()
  term?.focus()
}

function pushStatus(next: SessionStatus, lat = 0) {
  status.value = next
  emit("status", props.sessionKey, next, lat)
}

function setOffline(reason: string) {
  if (disposed) return
  pushStatus("offline")
  term?.write(`\r\n\x1b[31m[Dc Xshell] ${reason}\x1b[0m\r\n`)
}

async function openSessionOnce() {
  const info = await api.openSession(
    props.sessionKey,
    props.connectionId,
    term?.cols || 100,
    term?.rows || 30
  )
  currentSessionId = info.sessionId
}

async function connectInitial() {
  pushStatus("connecting")
  try {
    await openSessionOnce()
    if (disposed) return
    pushStatus("online")
    latency.value = 0
    if (props.active) term?.focus()
  } catch (err) {
    setOffline(String(err))
    ElMessage.error(String(err))
  }
}

function startHeartbeat() {
  stopHeartbeat()
  heartbeatTimer = window.setInterval(() => {
    void ping()
  }, 5000)
}

function stopHeartbeat() {
  if (heartbeatTimer !== null) {
    window.clearInterval(heartbeatTimer)
    heartbeatTimer = null
  }
}

async function ping() {
  if (disposed || reconnecting || status.value !== "online") return
  try {
    const res = await api.pingSession(props.sessionKey)
    if (disposed) return
    if (res.alive) {
      latency.value = res.latencyMs
      emit("status", props.sessionKey, "online", res.latencyMs)
    } else {
      handleLost(res.message || "心跳检测失败")
    }
  } catch (err) {
    handleLost(String(err))
  }
}

function handleLost(reason: string) {
  if (disposed || reconnecting || status.value === "offline") return
  term?.write(`\r\n\x1b[33m[Dc Xshell] ${reason}，开始自动重连...\x1b[0m\r\n`)
  pushStatus("reconnecting")
  void attemptReconnect()
}

async function attemptReconnect() {
  if (disposed || reconnecting) return
  reconnecting = true
  let attempt = 0
  while (attempt < 3 && !disposed) {
    attempt += 1
    pushStatus("reconnecting")
    term?.write(`\x1b[33m[Dc Xshell] 正在第 ${attempt}/3 次重连...\x1b[0m\r\n`)
    await sleep(1200)
    if (disposed) return
    try {
      await openSessionOnce()
      if (disposed) return
      reconnecting = false
      pushStatus("online")
      latency.value = 0
      term?.write(`\x1b[32m[Dc Xshell] 重连成功，会话已恢复\x1b[0m\r\n`)
      if (props.active) term?.focus()
      return
    } catch (err) {
      term?.write(`\x1b[90m[Dc Xshell] 第 ${attempt} 次重连失败：${err}\x1b[0m\r\n`)
    }
  }
  reconnecting = false
  if (!disposed) setOffline("连续 3 次重连均失败，卡片已标记为掉线")
}

async function manualReconnect() {
  if (reconnecting) return
  reconnecting = false
  pushStatus("reconnecting")
  term?.write("\r\n\x1b[36m[Dc Xshell] 手动发起重连...\x1b[0m\r\n")
  await attemptReconnect()
}

function onContextMenu(event: MouseEvent) {
  event.preventDefault()
  ctx.hasSelection = term?.hasSelection() ?? false
  ctx.visible = true
  ctx.x = Math.min(event.clientX, window.innerWidth - 190)
  ctx.y = Math.min(event.clientY, window.innerHeight - 250)
  term?.focus()
}

function closeCtx() {
  ctx.visible = false
}

async function runCtx(action: "copy" | "paste" | "selectAll" | "clear" | "reconnect") {
  closeCtx()
  if (action === "copy") {
    const ok = await copySelection()
    if (ok) ElMessage.success("已复制选中内容")
  }
  if (action === "paste") await pasteClipboard()
  if (action === "selectAll") term?.selectAll()
  if (action === "clear") clearScreen()
  if (action === "reconnect") await manualReconnect()
}

async function bindSessionEvents() {
  unOutput = await listen<SessionOutputEvent>("dcx:output", (event) => {
    if (disposed || event.payload.clientKey !== props.sessionKey) return
    try {
      term?.write(event.payload.data)
    } catch {
      /* ignore */
    }
  })

  unClosed = await listen<SessionClosedEvent>("dcx:closed", (event) => {
    const payload = event.payload
    if (disposed || payload.clientKey !== props.sessionKey) return
    // 旧会话（重连前）的关闭事件直接忽略
    if (payload.sessionId !== currentSessionId) return
    if (payload.reason.includes("远端关闭")) {
      setOffline("远端关闭了会话，终端已退出")
      return
    }
    handleLost(payload.reason || "连接已断开")
  })
}

function observeResize() {
  if (!holder.value) return
  resizeObserver = new ResizeObserver(() => {
    if (resizeTimer !== null) window.clearTimeout(resizeTimer)
    resizeTimer = window.setTimeout(() => {
      if (props.active) fit()
    }, 60)
  })
  resizeObserver.observe(holder.value)
}

watch(
  () => props.active,
  async (active) => {
    if (!active) return
    await nextTick()
    setTimeout(() => {
      fit()
      term?.focus()
    }, 30)
  }
)

onMounted(async () => {
  await nextTick()
  if (!holder.value) return
  initTerminal()
  document.addEventListener("click", closeCtx)
  await bindSessionEvents()
  startHeartbeat()
  observeResize()
  await connectInitial()
})

onBeforeUnmount(() => {
  disposed = true
  stopHeartbeat()
  if (resizeTimer !== null) window.clearTimeout(resizeTimer)
  resizeObserver?.disconnect()
  document.removeEventListener("click", closeCtx)
  unOutput?.()
  unClosed?.()
  api.closeSession(props.sessionKey).catch(() => {
    /* ignore */
  })
  try {
    webglAddon?.dispose()
  } catch {
    /* ignore */
  }
  term?.dispose()
  term = null
})
</script>

<template>
  <div class="session" :class="[status, { active: props.active }]">
    <header class="s-head">
      <div class="s-left">
        <span class="s-pill" :class="status">
          <span class="p-dot"></span>
          {{ statusLabel }}
        </span>
        <div class="s-title">
          <h2>{{ name }}</h2>
          <span class="s-sub">{{ username }}@{{ host }}:{{ port }}</span>
        </div>
      </div>
      <div class="s-right">
        <span v-if="status === 'online'" class="latency">心跳 {{ latency }} ms</span>
        <button
          class="s-btn"
          title="文件传输：上传 / 下载"
          :disabled="status !== 'online'"
          @click="showFiles = true"
        >
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
            <path
              d="M4 7.5A2.5 2.5 0 016.5 5h3l1.8 2H17A2 2 0 0119 9v1M4 7.5V17a2 2 0 002 2h6"
              stroke="currentColor"
              stroke-width="1.6"
              stroke-linecap="round"
            />
            <path d="M14 19v-6m0 0l-2.2 2.2M14 13l2.2 2.2" stroke="currentColor" stroke-width="1.6" />
          </svg>
        </button>
        <button class="s-btn" title="重新连接" @click="manualReconnect">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
            <path
              d="M20 11a8 8 0 10-2.3 5.7M20 5v6h-6"
              stroke="currentColor"
              stroke-width="1.7"
              stroke-linecap="round"
            />
          </svg>
        </button>
        <button class="s-btn" title="清屏" @click="clearScreen">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
            <path d="M4 7h16M6 7l1 12h10l1-12M9 7V5h6v2" stroke="currentColor" stroke-width="1.6" />
          </svg>
        </button>
        <button class="s-btn danger" title="关闭窗口" @click="emit('close', sessionKey)">
          <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
            <path d="M6 6l12 12M18 6L6 18" stroke="currentColor" stroke-width="1.7" />
          </svg>
        </button>
      </div>
    </header>

    <div class="term-shell" :class="{ offline: status === 'offline' }" @contextmenu="onContextMenu">
      <div ref="holder" class="term-holder"></div>
      <div v-if="status === 'offline'" class="offline-flag">
        <span class="offline-dot"></span>
        连接已断开 · 可点击右上角重连
      </div>
    </div>

    <div
      v-if="ctx.visible"
      class="ctx-menu"
      :style="{ left: ctx.x + 'px', top: ctx.y + 'px' }"
      @click.stop
    >
      <button :disabled="!ctx.hasSelection" @click="runCtx('copy')">
        复制<span class="sc">Ctrl+C</span>
      </button>
      <button @click="runCtx('paste')">粘贴<span class="sc">Ctrl+V</span></button>
      <button @click="runCtx('selectAll')">全选</button>
      <button @click="runCtx('clear')">清屏</button>
      <div class="sep"></div>
      <button @click="runCtx('reconnect')">重新连接</button>
    </div>

    <FileTransferDialog
      v-model="showFiles"
      :client-key="sessionKey"
      :host="host"
      :username="username"
    />
  </div>
</template>

<style scoped>
.session {
  position: absolute;
  inset: 0;
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px 12px 12px;
  background: radial-gradient(900px 420px at 20% -8%, rgba(45, 212, 191, 0.05), transparent 60%),
    var(--bg-0);
}

.s-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 14px;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #101a21, #0c141a);
  box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.03);
  transition: border-color 0.2s ease, background 0.2s ease;
}

.session.reconnecting .s-head {
  border-color: rgba(251, 191, 36, 0.35);
}

.session.offline .s-head {
  border-color: rgba(248, 113, 113, 0.55);
  background: linear-gradient(180deg, #1c1114, #120c0e);
  box-shadow: 0 0 26px rgba(248, 113, 113, 0.08) inset;
}

.s-left {
  display: flex;
  align-items: center;
  gap: 12px;
  min-width: 0;
}

.s-pill {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 22px;
  padding: 0 9px;
  border-radius: 999px;
  font-size: 11px;
  letter-spacing: 0.3px;
  border: 1px solid transparent;
  flex: 0 0 auto;
}

.s-pill .p-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
}

.s-pill.connecting {
  background: rgba(96, 165, 250, 0.1);
  border-color: rgba(96, 165, 250, 0.3);
  color: #93c5fd;
}

.s-pill.connecting .p-dot {
  background: #93c5fd;
}

.s-pill.online {
  background: rgba(52, 211, 153, 0.1);
  border-color: rgba(52, 211, 153, 0.3);
  color: #6ee7b7;
}

.s-pill.online .p-dot {
  background: #34d399;
  box-shadow: 0 0 8px rgba(52, 211, 153, 0.8);
}

.s-pill.reconnecting {
  background: rgba(251, 191, 36, 0.1);
  border-color: rgba(251, 191, 36, 0.32);
  color: #fcd34d;
}

.s-pill.reconnecting .p-dot {
  background: #fbbf24;
  animation: pulse 1s ease-in-out infinite;
}

.s-pill.offline {
  background: rgba(248, 113, 113, 0.12);
  border-color: rgba(248, 113, 113, 0.38);
  color: #fca5a5;
}

.s-pill.offline .p-dot {
  background: #f87171;
  box-shadow: 0 0 8px rgba(248, 113, 113, 0.8);
}

@keyframes pulse {
  50% {
    opacity: 0.35;
  }
}

.s-title {
  display: flex;
  align-items: baseline;
  gap: 10px;
  min-width: 0;
}

.s-title h2 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: #e8f4f1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  max-width: 380px;
}

.s-sub {
  font-family: var(--font-mono);
  font-size: 11px;
  color: #5e7a87;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.s-right {
  margin-left: auto;
  display: flex;
  align-items: center;
  gap: 6px;
  flex: 0 0 auto;
}

.latency {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: #4d7d76;
  margin-right: 6px;
}

.s-btn {
  width: 30px;
  height: 30px;
  display: grid;
  place-items: center;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: #75909d;
  cursor: pointer;
  transition: all 0.15s ease;
}

.s-btn:hover {
  background: #16242c;
  border-color: #22343e;
  color: #cfe3ec;
}

.s-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.s-btn:disabled:hover {
  background: transparent;
  border-color: transparent;
  color: #75909d;
}

.s-btn.danger:hover {
  background: rgba(248, 113, 113, 0.12);
  border-color: rgba(248, 113, 113, 0.35);
  color: #ff9c9c;
}

.term-shell {
  position: relative;
  flex: 1;
  min-height: 0;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: #080d11;
  overflow: hidden;
  transition: border-color 0.25s ease, box-shadow 0.25s ease;
}

.term-shell:focus-within {
  border-color: #263a46;
}

.session.offline .term-shell {
  border-color: rgba(248, 113, 113, 0.5);
  box-shadow: inset 0 0 60px rgba(248, 113, 113, 0.05), 0 0 22px rgba(248, 113, 113, 0.07);
}

.term-holder {
  position: absolute;
  inset: 8px 4px 6px 10px;
}

.term-holder :deep(.xterm) {
  height: 100%;
}

.offline-flag {
  position: absolute;
  right: 12px;
  bottom: 10px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  padding: 5px 10px;
  border-radius: 7px;
  background: rgba(40, 12, 14, 0.85);
  border: 1px solid rgba(248, 113, 113, 0.35);
  color: #fca5a5;
  font-size: 11px;
  backdrop-filter: blur(4px);
  pointer-events: none;
}

.offline-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #f87171;
  box-shadow: 0 0 8px rgba(248, 113, 113, 0.8);
}

.ctx-menu {
  position: fixed;
  z-index: 9999;
  min-width: 178px;
  padding: 5px;
  border-radius: 9px;
  background: #101b22;
  border: 1px solid #263842;
  box-shadow: 0 18px 44px rgba(0, 0, 0, 0.6);
}

.ctx-menu button {
  display: flex;
  align-items: center;
  justify-content: space-between;
  width: 100%;
  padding: 7px 10px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #b7c9d3;
  font-size: 12px;
  text-align: left;
  cursor: pointer;
}

.ctx-menu button:hover:not(:disabled) {
  background: #17262f;
  color: #e6f4fb;
}

.ctx-menu button:disabled {
  opacity: 0.38;
  cursor: default;
}

.ctx-menu .sc {
  font-family: var(--font-mono);
  font-size: 10px;
  color: #4f6572;
}

.ctx-menu .sep {
  height: 1px;
  margin: 4px 6px;
  background: #1d2b34;
}
</style>
