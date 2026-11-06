<script setup lang="ts">
import { computed, onMounted, ref } from "vue"
import { ElMessage, ElMessageBox } from "element-plus"
import Sidebar from "@/components/Sidebar.vue"
import SessionView from "@/components/SessionView.vue"
import ConnectionDialog from "@/components/ConnectionDialog.vue"
import * as api from "@/api"
import { appState, GUEST_LIMIT } from "@/store"
import type { ConnectionItem, SessionStatus } from "@/types"

interface Tab {
  key: string
  connectionId: number
  name: string
  host: string
  port: number
  username: string
  status: SessionStatus
  latency: number
}

const emit = defineEmits<{
  loginRequest: []
  logout: []
}>()

const tabs = ref<Tab[]>([])
const activeKey = ref("")
const dialogVisible = ref(false)
const editing = ref<ConnectionItem | null>(null)

const onlineCount = computed(() => tabs.value.filter((t) => t.status === "online").length)

const statusByConnection = computed<Record<number, SessionStatus>>(() => {
  const rank: Record<SessionStatus, number> = {
    online: 0,
    connecting: 1,
    reconnecting: 2,
    offline: 3,
  }
  const map: Record<number, SessionStatus> = {}
  for (const tab of tabs.value) {
    const current = map[tab.connectionId]
    if (!current || rank[tab.status] > rank[current]) {
      map[tab.connectionId] = tab.status
    }
  }
  return map
})

async function refreshConnections() {
  try {
    appState.connections = await api.listConnections()
  } catch (err) {
    ElMessage.error(`读取连接列表失败：${err}`)
  }
}

function makeKey() {
  const raw =
    window.crypto?.randomUUID?.() ??
    `t-${Date.now()}-${Math.random().toString(36).slice(2, 8)}`
  return raw.replace(/-/g, "")
}

async function openConnection(conn: ConnectionItem) {
  const existing = tabs.value.find((t) => t.connectionId === conn.id)
  if (existing) {
    activeKey.value = existing.key
    return
  }

  if (appState.user === null && tabs.value.length >= GUEST_LIMIT) {
    try {
      await ElMessageBox.confirm(
        `访客模式最多同时打开 ${GUEST_LIMIT} 个并行窗口，登录后可无限打开。`,
        "窗口数量已达上限",
        {
          confirmButtonText: "去登录",
          cancelButtonText: "知道了",
          type: "warning",
        }
      )
      emit("loginRequest")
    } catch {
      /* 用户选择知道了 */
    }
    return
  }

  const key = makeKey()
  tabs.value.push({
    key,
    connectionId: conn.id,
    name: conn.name,
    host: conn.host,
    port: conn.port,
    username: conn.username,
    status: "connecting",
    latency: 0,
  })
  activeKey.value = key
}

function closeTab(key: string) {
  const index = tabs.value.findIndex((t) => t.key === key)
  if (index === -1) return
  tabs.value.splice(index, 1)
  if (activeKey.value === key) {
    const next = tabs.value[index - 1] ?? tabs.value[index] ?? tabs.value[0]
    activeKey.value = next ? next.key : ""
  }
}

function onSessionStatus(key: string, status: SessionStatus, latency: number) {
  const tab = tabs.value.find((t) => t.key === key)
  if (!tab) return
  tab.status = status
  tab.latency = latency
}

function openNew() {
  editing.value = null
  dialogVisible.value = true
}

function openEdit(conn: ConnectionItem) {
  editing.value = conn
  dialogVisible.value = true
}

async function onSaved() {
  await refreshConnections()
}

async function removeConnection(conn: ConnectionItem) {
  try {
    await ElMessageBox.confirm(
      `确定删除连接「${conn.name}」吗？本地保存的配置与密码将一并删除。`,
      "删除连接",
      { confirmButtonText: "删除", cancelButtonText: "取消", type: "warning" }
    )
  } catch {
    return
  }
  try {
    await api.deleteConnection(conn.id)
    const keys = tabs.value.filter((t) => t.connectionId === conn.id).map((t) => t.key)
    for (const key of keys) closeTab(key)
    await refreshConnections()
    ElMessage.success("连接已删除")
  } catch (err) {
    ElMessage.error(String(err))
  }
}

async function handleLogout() {
  try {
    await api.logout()
    emit("logout")
  } catch (err) {
    ElMessage.error(String(err))
  }
}

onMounted(refreshConnections)
</script>

<template>
  <div class="workspace">
    <Sidebar
      :connections="appState.connections"
      :status-by-connection="statusByConnection"
      :opened-count="tabs.length"
      :user="appState.user"
      @open="openConnection"
      @new="openNew"
      @edit="openEdit"
      @remove="removeConnection"
      @refresh="refreshConnections"
      @login-request="emit('loginRequest')"
      @logout="handleLogout"
    />

    <main class="main">
      <div v-if="tabs.length" class="tabbar">
        <div class="tabs-scroll">
          <div
            v-for="tab in tabs"
            :key="tab.key"
            class="tab"
            :class="[tab.status, { active: tab.key === activeKey }]"
            @click="activeKey = tab.key"
          >
            <span class="t-dot" :class="tab.status"></span>
            <span class="t-name">{{ tab.name }}</span>
            <span v-if="tab.status === 'offline'" class="t-tag">掉线</span>
            <button class="t-close" title="关闭窗口" @click.stop="closeTab(tab.key)">
              <svg width="10" height="10" viewBox="0 0 10 10">
                <path d="M1.5 1.5l7 7M8.5 1.5l-7 7" stroke="currentColor" stroke-width="1.3" />
              </svg>
            </button>
          </div>
        </div>
        <div class="tabbar-right">
          <span class="win-count">
            窗口 {{ tabs.length }}<template v-if="appState.user"> · 无限</template>
            <template v-else> / {{ GUEST_LIMIT }}</template>
          </span>
        </div>
      </div>

      <div class="stage">
        <template v-if="tabs.length">
          <SessionView
            v-for="tab in tabs"
            v-show="tab.key === activeKey"
            :key="tab.key"
            :session-key="tab.key"
            :connection-id="tab.connectionId"
            :name="tab.name"
            :host="tab.host"
            :port="tab.port"
            :username="tab.username"
            :active="tab.key === activeKey"
            @status="onSessionStatus"
            @close="closeTab"
          />
        </template>

        <div v-else class="welcome">
          <div class="w-hero">
            <div class="w-eyebrow">SSH WORKSPACE</div>
            <h1>连接你的服务器</h1>
            <p>
              从左侧连接树选择服务器，或使用下方快速入口。掉线会自动重连 3 次，
              仍失败时卡片将变红提示。
            </p>
            <div class="w-actions">
              <button class="primary-btn" @click="openNew">
                <svg width="14" height="14" viewBox="0 0 24 24" fill="none">
                  <path d="M12 5v14M5 12h14" stroke="currentColor" stroke-width="1.8" />
                </svg>
                新建连接
              </button>
              <span v-if="appState.user === null" class="w-note">
                访客模式 · 最多 {{ GUEST_LIMIT }} 个并行窗口
              </span>
              <button v-else class="w-note link" @click="emit('loginRequest')">账号已登录 · 无限窗口</button>
            </div>
          </div>

          <div class="w-stats">
            <div class="stat">
              <span class="label">连接配置</span>
              <span class="value">{{ appState.connections.length }}</span>
            </div>
            <div class="stat">
              <span class="label">并行窗口</span>
              <span class="value">{{ tabs.length }}</span>
            </div>
            <div class="stat">
              <span class="label">在线会话</span>
              <span class="value teal">{{ onlineCount }}</span>
            </div>
            <div class="stat">
              <span class="label">账号状态</span>
              <span class="value small">{{ appState.user ? appState.user.username : "访客" }}</span>
            </div>
          </div>

          <div class="w-section">
            <span>快速连接</span>
            <span class="w-hint" v-if="appState.connections.length === 0">
              还没有连接，点击「新建连接」开始
            </span>
            <span class="w-hint" v-else>点击卡片即可打开终端</span>
          </div>

          <div v-if="appState.connections.length" class="quick-grid">
            <div
              v-for="conn in appState.connections"
              :key="conn.id"
              class="quick-card"
              @click="openConnection(conn)"
            >
              <div class="q-top">
                <span class="q-dot" :class="statusByConnection[conn.id] || 'idle'"></span>
                <span class="q-group">{{ conn.groupName }}</span>
                <button class="q-edit" title="编辑" @click.stop="openEdit(conn)">
                  <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
                    <path d="M4 20h4l10-10-4-4L4 16v4z" stroke="currentColor" stroke-width="1.5" />
                    <path d="M14 6l4 4" stroke="currentColor" stroke-width="1.5" />
                  </svg>
                </button>
              </div>
              <h3>{{ conn.name }}</h3>
              <p class="q-host">{{ conn.username }}@{{ conn.host }}:{{ conn.port }}</p>
              <div class="q-foot">
                <span>{{ conn.authType === "key" ? "私钥认证" : "密码认证" }}</span>
                <span class="q-enter">打开 →</span>
              </div>
            </div>
          </div>
        </div>
      </div>
    </main>

    <ConnectionDialog
      v-model:visible="dialogVisible"
      :editing="editing"
      @saved="onSaved"
    />
  </div>
</template>

<style scoped>
.workspace {
  height: 100%;
  display: flex;
  min-height: 0;
  background: var(--bg-0);
}

.main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.tabbar {
  flex: 0 0 42px;
  display: flex;
  align-items: stretch;
  border-bottom: 1px solid var(--border);
  background: linear-gradient(180deg, #0b1318, #090f13);
}

.tabs-scroll {
  flex: 1;
  display: flex;
  overflow-x: auto;
  overflow-y: hidden;
  min-width: 0;
}

.tab {
  position: relative;
  display: flex;
  align-items: center;
  gap: 7px;
  padding: 0 10px 0 13px;
  border-right: 1px solid #16222a;
  max-width: 220px;
  cursor: pointer;
  color: #8fa5b1;
  transition: background 0.15s ease, color 0.15s ease;
  flex: 0 0 auto;
}

.tab:hover {
  background: #0e181e;
}

.tab.active {
  background: #101c23;
  color: #d9e9f1;
}

.tab.active::after {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  bottom: -1px;
  height: 2px;
  background: var(--accent);
  box-shadow: 0 0 10px rgba(45, 212, 191, 0.6);
}

.tab.offline.active::after {
  background: var(--red);
  box-shadow: 0 0 10px rgba(248, 113, 113, 0.6);
}

.t-dot {
  width: 6px;
  height: 6px;
  flex: 0 0 6px;
  border-radius: 50%;
  background: #2c404b;
}

.t-dot.connecting {
  background: #60a5fa;
}

.t-dot.online {
  background: var(--green);
  box-shadow: 0 0 8px rgba(52, 211, 153, 0.7);
}

.t-dot.reconnecting {
  background: var(--amber);
  animation: tabBlink 1s ease-in-out infinite;
}

.t-dot.offline {
  background: var(--red);
  box-shadow: 0 0 8px rgba(248, 113, 113, 0.6);
}

@keyframes tabBlink {
  50% {
    opacity: 0.35;
  }
}

.t-name {
  font-size: 12.5px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.t-tag {
  font-size: 10px;
  color: #fca5a5;
  border: 1px solid rgba(248, 113, 113, 0.35);
  background: rgba(248, 113, 113, 0.08);
  border-radius: 4px;
  padding: 1px 4px;
  flex: 0 0 auto;
}

.t-close {
  width: 19px;
  height: 19px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: #566b77;
  cursor: pointer;
  flex: 0 0 auto;
}

.t-close:hover {
  background: rgba(248, 113, 113, 0.14);
  color: #ff9c9c;
}

.tabbar-right {
  display: flex;
  align-items: center;
  padding: 0 16px;
  border-left: 1px solid #16222a;
  flex: 0 0 auto;
}

.win-count {
  font-size: 11px;
  color: #5b7280;
  font-family: var(--font-mono);
}

.stage {
  position: relative;
  flex: 1;
  min-height: 0;
}

.welcome {
  height: 100%;
  overflow-y: auto;
  padding: 38px 40px 34px;
  background: radial-gradient(1100px 480px at 30% -10%, rgba(45, 212, 191, 0.07), transparent 62%),
    radial-gradient(900px 520px at 92% 12%, rgba(56, 189, 248, 0.06), transparent 60%),
    var(--bg-0);
  animation: fade 0.35s ease both;
}

@keyframes fade {
  from {
    opacity: 0;
  }
  to {
    opacity: 1;
  }
}

.w-hero {
  max-width: 640px;
}

.w-eyebrow {
  font-family: var(--font-mono);
  font-size: 10px;
  letter-spacing: 3px;
  color: #3f8f86;
  margin-bottom: 12px;
}

.w-hero h1 {
  margin: 0;
  font-size: 30px;
  font-weight: 700;
  letter-spacing: 0.5px;
  background: linear-gradient(100deg, #eafcf8 20%, #57d9d0 75%, #38bdf8);
  -webkit-background-clip: text;
  background-clip: text;
  color: transparent;
}

.w-hero p {
  margin: 12px 0 22px;
  font-size: 12.5px;
  line-height: 1.9;
  color: var(--text-dim);
}

.w-actions {
  display: flex;
  align-items: center;
  gap: 14px;
}

.primary-btn {
  height: 38px;
  padding: 0 18px;
  display: inline-flex;
  align-items: center;
  gap: 7px;
  border-radius: 9px;
  border: 1px solid rgba(45, 212, 191, 0.5);
  background: linear-gradient(180deg, rgba(45, 212, 191, 0.2), rgba(45, 212, 191, 0.08));
  color: #a7fff2;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.18s ease;
}

.primary-btn:hover {
  background: rgba(45, 212, 191, 0.25);
  color: #e8fffb;
  box-shadow: 0 0 28px rgba(45, 212, 191, 0.16);
}

.w-note {
  font-size: 11.5px;
  color: var(--text-mute);
}

.w-note.link {
  border: 0;
  background: transparent;
  cursor: pointer;
  font-size: 11.5px;
  color: #4ecdc0;
}

.w-stats {
  margin-top: 34px;
  display: grid;
  grid-template-columns: repeat(4, minmax(0, 1fr));
  gap: 12px;
  max-width: 760px;
}

.stat {
  padding: 16px 18px;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0f1a20, #0b1318);
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.stat .label {
  font-size: 11px;
  color: var(--text-mute);
  letter-spacing: 0.4px;
}

.stat .value {
  font-family: var(--font-mono);
  font-size: 24px;
  font-weight: 600;
  color: #dceaf1;
  line-height: 1;
}

.stat .value.teal {
  color: var(--accent);
}

.stat .value.small {
  font-size: 15px;
  line-height: 1.55;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.w-section {
  margin: 34px 0 14px;
  display: flex;
  align-items: center;
  gap: 12px;
  font-size: 12px;
  color: #a9c0cb;
  letter-spacing: 0.5px;
}

.w-section::after {
  content: "";
  flex: 1;
  height: 1px;
  background: linear-gradient(90deg, #1b2930, transparent);
}

.w-hint {
  font-size: 11px;
  color: var(--text-mute);
  order: 3;
  letter-spacing: 0;
}

.quick-grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(250px, 1fr));
  gap: 12px;
  max-width: 1180px;
}

.quick-card {
  position: relative;
  padding: 16px;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0e181e, #0a1116);
  cursor: pointer;
  transition: all 0.18s ease;
  overflow: hidden;
}

.quick-card::before {
  content: "";
  position: absolute;
  inset: 0;
  background: radial-gradient(220px 120px at 20% 0%, rgba(45, 212, 191, 0.09), transparent 70%);
  opacity: 0;
  transition: opacity 0.2s ease;
}

.quick-card:hover {
  border-color: rgba(45, 212, 191, 0.34);
  transform: translateY(-2px);
  box-shadow: 0 12px 30px rgba(0, 0, 0, 0.35);
}

.quick-card:hover::before {
  opacity: 1;
}

.q-top {
  display: flex;
  align-items: center;
  gap: 7px;
  position: relative;
}

.q-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  background: #31434d;
}

.q-dot.online {
  background: var(--green);
  box-shadow: 0 0 7px rgba(52, 211, 153, 0.7);
}

.q-dot.reconnecting {
  background: var(--amber);
}

.q-dot.offline {
  background: var(--red);
}

.q-group {
  font-size: 10.5px;
  color: #54707d;
  border: 1px solid #1d2c35;
  border-radius: 5px;
  padding: 1px 6px;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.q-edit {
  margin-left: auto;
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: #4d6470;
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.15s ease, color 0.15s ease;
}

.quick-card:hover .q-edit {
  opacity: 1;
}

.q-edit:hover {
  background: #17262f;
  color: #b9d6e2;
}

.quick-card h3 {
  position: relative;
  margin: 12px 0 6px;
  font-size: 14px;
  font-weight: 600;
  color: #dceaf2;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.q-host {
  position: relative;
  margin: 0;
  font-family: var(--font-mono);
  font-size: 11px;
  color: #5f7b88;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.q-foot {
  position: relative;
  margin-top: 14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  font-size: 10.5px;
  color: #4f6773;
}

.q-enter {
  color: #3f9d93;
  transition: color 0.15s ease;
}

.quick-card:hover .q-enter {
  color: var(--accent);
}
</style>
