<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from "vue"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { open } from "@tauri-apps/plugin-dialog"
import { ElMessage } from "element-plus"
import { Download, Refresh, Upload } from "@element-plus/icons-vue"
import * as api from "@/api"
import { t, tErr } from "@/i18n"
import type { LanIncomingEvent, LanPeer, LanStatus, LanTransferEvent } from "@/types"

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{ "update:visible": [boolean] }>()

const status = ref<LanStatus | null>(null)
const peers = ref<LanPeer[]>([])
const transfers = ref<LanTransferEvent[]>([])
const incoming = ref<LanIncomingEvent | null>(null)
const saveDir = ref("")
/** 本次会话是否手动改过接收目录；未改过时跟随设置里的默认目录 */
const customDir = ref(false)
const refreshing = ref(false)
const sendingTo = ref("")

let unPeer: UnlistenFn | null = null
let unTransfer: UnlistenFn | null = null
let unIncoming: UnlistenFn | null = null

const listening = computed(() => status.value?.listening ?? false)

async function loadStatus() {
  try {
    status.value = await api.lanStatus()
    if (!customDir.value) saveDir.value = status.value.saveDir
  } catch (err) {
    ElMessage.error(t("transfer.statusLoadFailed", { error: tErr(err) }))
  }
}

async function loadPeers() {
  try {
    peers.value = await api.lanPeers()
  } catch {
    /* 忽略：下次事件或刷新会补齐 */
  }
}

async function refresh() {
  refreshing.value = true
  await Promise.all([loadStatus(), loadPeers()])
  refreshing.value = false
}

function upsert(event: LanTransferEvent) {
  const index = transfers.value.findIndex((item) => item.id === event.id)
  if (index >= 0) {
    transfers.value[index] = { ...transfers.value[index], ...event }
  } else {
    transfers.value.unshift({ ...event })
  }
}

/** 访客身份在后端以 "guest" 标识，界面上按当前语言显示 */
function peerLabel(name: string) {
  const value = (name || "").trim()
  return value === "guest" || value === "访客" ? t("guest") : value
}

async function sendFile(peer: LanPeer) {
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      title: t("transfer.sendPickTitle", { name: peerLabel(peer.name) }),
    })
    if (!picked || Array.isArray(picked)) return
    sendingTo.value = peer.id
    await api.lanSendFile(picked, peer.id)
  } catch (err) {
    ElMessage.error(t("transfer.sendFailed", { error: tErr(err) }))
  } finally {
    sendingTo.value = ""
  }
}

async function pickDir() {
  try {
    const dir = await open({ directory: true, title: t("transfer.savePickTitle") })
    if (typeof dir === "string") {
      saveDir.value = dir
      customDir.value = true
    }
  } catch {
    /* 用户取消选择 */
  }
}

async function acceptIncoming() {
  const item = incoming.value
  if (!item) return
  incoming.value = null
  try {
    await api.lanRespond(item.id, true, saveDir.value || null)
    upsert({
      id: item.id,
      direction: "receive",
      peer: item.from,
      fileName: item.fileName,
      total: item.size,
      transferred: 0,
      status: "waiting",
      message: "transfer.preparing",
    })
  } catch (err) {
    ElMessage.error(t("transfer.receiveFailed", { error: tErr(err) }))
  }
}

async function declineIncoming() {
  const item = incoming.value
  if (!item) return
  incoming.value = null
  try {
    await api.lanRespond(item.id, false, null)
  } catch (err) {
    ElMessage.error(tErr(err))
  }
}

function fmtSize(value: number) {
  if (!value) return "0 B"
  const units = ["B", "KB", "MB", "GB", "TB"]
  let size = value
  let index = 0
  while (size >= 1024 && index < units.length - 1) {
    size /= 1024
    index += 1
  }
  return `${index === 0 || size >= 100 ? size.toFixed(0) : size.toFixed(1)} ${units[index]}`
}

function percent(item: LanTransferEvent) {
  if (!item.total) return item.status === "done" ? 100 : 0
  return Math.min(100, Math.round((item.transferred / item.total) * 100))
}

async function openSaved(item: LanTransferEvent) {
  if (!item.path) return
  try {
    await api.openPath(item.path)
  } catch (err) {
    ElMessage.error(t("transfer.openDirFailed", { error: tErr(err) }))
  }
}

function statusText(item: LanTransferEvent) {
  switch (item.status) {
    case "waiting":
      return item.direction === "send"
        ? t("transfer.status.waitConfirm")
        : t("transfer.status.waitStart")
    case "active":
      return item.direction === "send"
        ? t("transfer.status.sending")
        : t("transfer.status.receiving")
    case "done":
      return t("transfer.status.done")
    case "declined":
      return t("transfer.status.declined")
    default:
      return t("transfer.status.failed")
  }
}

onMounted(async () => {
  await refresh()

  unPeer = await listen<LanPeer[]>("dcx:peers", (event) => {
    peers.value = event.payload
  })

  unTransfer = await listen<LanTransferEvent>("dcx:transfer", (event) => {
    upsert(event.payload)
  })

  unIncoming = await listen<LanIncomingEvent>("dcx:incoming", (event) => {
    incoming.value = event.payload
    if (!saveDir.value) saveDir.value = status.value?.saveDir ?? ""
    emit("update:visible", true)
  })
})

onBeforeUnmount(() => {
  unPeer?.()
  unTransfer?.()
  unIncoming?.()
})

defineExpose({ refresh })
</script>

<template>
  <section v-if="props.visible" class="transfer-page">
    <header class="tr-head">
      <button class="back-btn" :title="t('transfer.backTitle')" @click="emit('update:visible', false)">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
          <path
            d="M15 5l-7 7 7 7"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        {{ t("transfer.back") }}
      </button>
      <div class="head-text">
        <h2>{{ t("transfer.title") }}</h2>
        <p v-if="status">
          {{ t("transfer.localName", { name: peerLabel(status.name) || t("transfer.inactive") }) }} ·
          {{ status.ip || t("transfer.noIp") }} ·
          {{ t("transfer.port", { port: status.port }) }}
        </p>
      </div>
      <button class="refresh-btn" :disabled="refreshing" @click="refresh">
        <el-icon><Refresh /></el-icon>
        {{ refreshing ? t("transfer.refreshing") : t("transfer.refresh") }}
      </button>
    </header>

    <div v-if="status && !listening" class="warn">
      {{ t("transfer.warn") }}
    </div>

    <div v-if="status?.saveDirFallback" class="warn">
      {{ t("settings.transfer.fallback") }}
    </div>

    <div class="save-line">
      <span class="save-label">{{ t("transfer.saveTo") }}</span>
      <span class="save-path mono" :title="saveDir">{{ saveDir || t("transfer.unset") }}</span>
      <button class="link-btn" @click="pickDir">{{ t("transfer.changeDir") }}</button>
    </div>

    <div class="tr-body">
      <aside class="peer-pane">
        <div class="pane-title">
          {{ t("transfer.peers") }} <span class="count">{{ peers.length }}</span>
        </div>
        <div v-if="peers.length === 0" class="peer-empty">
          {{ t("transfer.peersEmpty") }}<br />{{ t("transfer.peersEmpty2") }}
        </div>
        <div v-for="peer in peers" :key="peer.id" class="peer-item">
          <span class="p-avatar">{{ peerLabel(peer.name).slice(0, 1).toUpperCase() }}</span>
          <div class="peer-text">
            <span class="p-name" :title="peerLabel(peer.name)">{{ peerLabel(peer.name) }}</span>
            <span class="p-ip mono">{{ peer.ip }}</span>
          </div>
          <button class="send-btn" :disabled="sendingTo === peer.id" @click="sendFile(peer)">
            <el-icon><Upload /></el-icon>
            {{ t("transfer.send") }}
          </button>
        </div>
      </aside>

      <div class="record-pane">
        <div class="pane-title">
          {{ t("transfer.records") }} <span class="count">{{ transfers.length }}</span>
          <span class="pane-hint">{{ t("transfer.limitHint") }}</span>
        </div>
        <div v-if="transfers.length === 0" class="record-empty">&gt;_ {{ t("transfer.noRecords") }}</div>
        <div v-for="item in transfers" :key="item.id" class="record" :class="item.status">
          <div class="r-top">
            <span class="r-dir" :class="item.direction">
              <el-icon>
                <Upload v-if="item.direction === 'send'" />
                <Download v-else />
              </el-icon>
            </span>
            <span class="r-name" :title="item.fileName">{{ item.fileName }}</span>
            <span class="r-peer">{{ item.direction === "send" ? "→" : "←" }} {{ item.peer }}</span>
            <span class="r-state">{{ statusText(item) }}</span>
          </div>
          <div class="r-bar">
            <div class="fill" :style="{ width: percent(item) + '%' }"></div>
          </div>
          <div class="r-foot">
            <span class="mono">
              {{ fmtSize(item.transferred) }} / {{ fmtSize(item.total) }} · {{ percent(item) }}%
            </span>
            <span class="r-msg" :title="tErr(item.message)">{{ tErr(item.message) }}</span>
            <button
              v-if="item.status === 'done' && item.path"
              class="open-dir-btn"
              :title="t('transfer.openDirTitle')"
              @click="openSaved(item)"
            >
              {{ t("transfer.openDir") }}
            </button>
          </div>
        </div>
      </div>
    </div>
  </section>

  <div v-if="incoming" class="incoming-mask">
    <div class="incoming-card">
      <div class="ic-head">
        <span class="ic-badge">{{ t("transfer.incoming") }}</span>
        <span class="ic-from">{{ t("transfer.from", { name: incoming.from, ip: incoming.ip }) }}</span>
      </div>
      <div class="ic-file">
        <span class="ic-name" :title="incoming.fileName">{{ incoming.fileName }}</span>
        <span class="ic-size mono">{{ fmtSize(incoming.size) }}</span>
      </div>
      <div class="ic-dir">
        <span class="ic-label">{{ t("transfer.saveToShort") }}</span>
        <span class="ic-path mono" :title="saveDir">{{ saveDir || t("transfer.unset") }}</span>
        <button class="link-btn" @click="pickDir">{{ t("transfer.change") }}</button>
      </div>
      <div class="ic-foot">
        <button class="ic-btn ghost" @click="declineIncoming">{{ t("transfer.decline") }}</button>
        <button class="ic-btn primary" @click="acceptIncoming">{{ t("transfer.accept") }}</button>
      </div>
    </div>
  </div>
</template>

<style scoped>
.transfer-page {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: flex;
  flex-direction: column;
  background: radial-gradient(900px 420px at 24% -12%, rgba(56, 189, 248, 0.06), transparent 62%),
    var(--bg-0);
  animation: pageIn 0.22s ease both;
}

@keyframes pageIn {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.tr-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 20px 26px 12px;
}

.back-btn {
  display: inline-flex;
  align-items: center;
  gap: 5px;
  height: 32px;
  padding: 0 12px 0 9px;
  border-radius: 8px;
  border: 1px solid #24343e;
  background: #0d171d;
  color: #a8c1cc;
  font-size: 12.5px;
  cursor: pointer;
}

.back-btn:hover {
  border-color: rgba(45, 212, 191, 0.4);
  color: #e2fffb;
}

.head-text {
  flex: 1;
  min-width: 0;
}

.head-text h2 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #eaf6f3;
}

.head-text p {
  margin: 5px 0 0;
  font-size: 11.5px;
  color: var(--text-mute);
}

.refresh-btn {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 32px;
  padding: 0 13px;
  border-radius: 8px;
  border: 1px solid #24343e;
  background: transparent;
  color: var(--text-dim);
  font-size: 12.5px;
  cursor: pointer;
}

.refresh-btn:hover:not(:disabled) {
  border-color: rgba(45, 212, 191, 0.4);
  color: #d9fffa;
}

.refresh-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.warn {
  margin: 0 26px 10px;
  padding: 10px 14px;
  border-radius: 9px;
  border: 1px solid rgba(251, 191, 36, 0.32);
  background: rgba(251, 191, 36, 0.08);
  color: #f6d08d;
  font-size: 12px;
}

.save-line {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 26px 12px;
  font-size: 11.5px;
  color: var(--text-mute);
}

.save-label {
  color: #6b8492;
}

.save-path {
  max-width: 56%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: #9fbdcb;
}

.link-btn {
  border: 0;
  background: transparent;
  color: #4ecdc0;
  font-size: 11.5px;
  cursor: pointer;
  padding: 0;
}

.link-btn:hover {
  color: #7ff2e2;
}

.tr-body {
  flex: 1;
  min-height: 0;
  display: flex;
  gap: 14px;
  padding: 0 26px 22px;
}

.peer-pane {
  flex: 0 0 300px;
  display: flex;
  flex-direction: column;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0d171d, #0a1116);
  overflow: hidden;
}

.record-pane {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0d171d, #0a1116);
  overflow-y: auto;
}

.pane-title {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 12px 14px;
  font-size: 12px;
  color: #a9c0cb;
  border-bottom: 1px solid #17252d;
}

.count {
  padding: 1px 6px;
  border-radius: 6px;
  background: #132029;
  color: #7d96a3;
  font-family: var(--font-mono);
  font-size: 10px;
}

.pane-hint {
  margin-left: auto;
  font-size: 11px;
  color: #4d6470;
}

.peer-empty {
  padding: 40px 18px;
  text-align: center;
  font-size: 12px;
  line-height: 1.9;
  color: var(--text-mute);
}

.peer-item {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 10px 14px;
  border-bottom: 1px solid #121d24;
}

.peer-item:hover {
  background: #101b22;
}

.p-avatar {
  width: 30px;
  height: 30px;
  flex: 0 0 30px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  background: linear-gradient(135deg, #10b981, #0ea5e9);
  color: #04120f;
  font-weight: 700;
  font-size: 13px;
}

.peer-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.p-name {
  font-size: 12.5px;
  color: #d6e6ee;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.p-ip {
  font-size: 11px;
  color: #6d8896;
}

.send-btn {
  flex: 0 0 auto;
  display: inline-flex;
  align-items: center;
  gap: 4px;
  height: 27px;
  padding: 0 10px;
  border-radius: 7px;
  border: 1px solid rgba(56, 189, 248, 0.35);
  background: rgba(56, 189, 248, 0.1);
  color: #a8d8f7;
  font-size: 11.5px;
  cursor: pointer;
}

.send-btn:hover:not(:disabled) {
  border-color: rgba(56, 189, 248, 0.6);
  background: rgba(56, 189, 248, 0.18);
  color: #dcf1ff;
}

.send-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.record-empty {
  padding: 60px 20px;
  text-align: center;
  font-family: var(--font-mono);
  font-size: 12px;
  color: #3f5663;
}

.record {
  padding: 12px 14px;
  border-bottom: 1px solid #121d24;
}

.record:hover {
  background: #101b22;
}

.r-top {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 12.5px;
}

.r-dir {
  width: 20px;
  height: 20px;
  flex: 0 0 20px;
  display: grid;
  place-items: center;
  border-radius: 6px;
  font-size: 12px;
}

.r-dir.send {
  color: #9cc8ff;
  background: rgba(96, 165, 250, 0.12);
}

.r-dir.receive {
  color: #7ee7c8;
  background: rgba(52, 211, 153, 0.12);
}

.r-name {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: #dceaf2;
}

.r-peer {
  flex: 0 0 auto;
  font-size: 11px;
  color: #6d8896;
}

.r-state {
  flex: 0 0 auto;
  font-size: 11px;
  color: #9fbdcb;
}

.record.active .r-state {
  color: #7ff2e2;
}

.record.done .r-state {
  color: var(--green);
}

.record.failed .r-state {
  color: var(--red);
}

.record.declined .r-state {
  color: #8fa5b1;
}

.r-bar {
  margin: 9px 0 7px;
  height: 4px;
  border-radius: 999px;
  background: #142029;
  overflow: hidden;
}

.fill {
  height: 100%;
  border-radius: 999px;
  background: linear-gradient(90deg, #0ea5e9, #2dd4bf);
  transition: width 0.18s ease;
}

.record.failed .fill {
  background: linear-gradient(90deg, #b91c1c, #f87171);
}

.record.done .fill {
  background: linear-gradient(90deg, #10b981, #34d399);
}

.r-foot {
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 11px;
  color: #5f7b88;
}

.r-msg {
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: #4f6773;
}

.open-dir-btn {
  flex: 0 0 auto;
  margin-left: auto;
  height: 22px;
  padding: 0 10px;
  border-radius: 5px;
  border: 1px solid rgba(45, 212, 191, 0.4);
  background: rgba(45, 212, 191, 0.1);
  color: #85f5e7;
  font-size: 11px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.open-dir-btn:hover {
  border-color: rgba(45, 212, 191, 0.75);
  background: rgba(45, 212, 191, 0.22);
  color: #e8fffb;
}

.mono {
  font-family: var(--font-mono);
}

.incoming-mask {
  position: fixed;
  inset: 0;
  z-index: 2000;
  display: grid;
  place-items: center;
  background: rgba(3, 8, 11, 0.62);
  backdrop-filter: blur(2px);
}

.incoming-card {
  width: 460px;
  max-width: calc(100vw - 60px);
  padding: 20px 22px 18px;
  border-radius: 14px;
  border: 1px solid rgba(45, 212, 191, 0.28);
  background: linear-gradient(180deg, #101c23, #0a1218);
  box-shadow: 0 24px 60px rgba(0, 0, 0, 0.55);
  animation: cardIn 0.2s ease both;
}

@keyframes cardIn {
  from {
    opacity: 0;
    transform: translateY(10px) scale(0.98);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.ic-head {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-bottom: 14px;
}

.ic-badge {
  padding: 2px 8px;
  border-radius: 6px;
  border: 1px solid rgba(45, 212, 191, 0.34);
  background: rgba(45, 212, 191, 0.1);
  color: #85f5e7;
  font-size: 11px;
}

.ic-from {
  font-size: 12px;
  color: #a8c1cc;
}

.ic-file {
  display: flex;
  flex-direction: column;
  gap: 5px;
  padding: 12px 14px;
  border-radius: 10px;
  border: 1px solid #1c2b34;
  background: #0b141a;
  margin-bottom: 12px;
}

.ic-name {
  font-size: 13px;
  color: #e4f2f8;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.ic-size {
  font-size: 11px;
  color: #6d8896;
}

.ic-dir {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11.5px;
  color: var(--text-mute);
  margin-bottom: 16px;
}

.ic-label {
  flex: 0 0 auto;
  color: #6b8492;
}

.ic-path {
  flex: 1;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  color: #9fbdcb;
}

.ic-foot {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
}

.ic-btn {
  height: 32px;
  padding: 0 18px;
  border-radius: 8px;
  font-size: 12.5px;
  cursor: pointer;
  border: 1px solid #263842;
  background: transparent;
  color: #b3c9d3;
}

.ic-btn:hover {
  border-color: #354d5a;
  color: #e2f4fb;
}

.ic-btn.primary {
  border-color: rgba(45, 212, 191, 0.5);
  background: linear-gradient(180deg, rgba(45, 212, 191, 0.2), rgba(45, 212, 191, 0.08));
  color: #a7fff2;
  font-weight: 600;
}

.ic-btn.primary:hover {
  background: rgba(45, 212, 191, 0.26);
  color: #e8fffb;
}
</style>
