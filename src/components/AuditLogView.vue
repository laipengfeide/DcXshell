<script setup lang="ts">
import { computed, onMounted, ref } from "vue"
import { Refresh } from "@element-plus/icons-vue"
import { ElMessage } from "element-plus"
import * as api from "@/api"
import { appState } from "@/store"
import type { AuditLogItem } from "@/types"

const emit = defineEmits<{ close: [] }>()

type Filter = "all" | "login" | "logout" | "connect" | "command"

const logs = ref<AuditLogItem[]>([])
const loading = ref(false)
const filter = ref<Filter>("all")
const ip = ref("")

const filters: { key: Filter; label: string }[] = [
  { key: "all", label: "全部" },
  { key: "login", label: "登录" },
  { key: "logout", label: "退出" },
  { key: "connect", label: "连接" },
  { key: "command", label: "命令" },
]

const meta: Record<string, { label: string; cls: string }> = {
  login: { label: "登录", cls: "login" },
  logout: { label: "退出", cls: "logout" },
  connect: { label: "连接", cls: "connect" },
  command: { label: "命令", cls: "command" },
}

const filtered = computed(() =>
  filter.value === "all" ? logs.value : logs.value.filter((item) => item.action === filter.value)
)

const identity = computed(() =>
  appState.user ? appState.user.username : "访客（本机访客记录）"
)

function countOf(key: Filter) {
  if (key === "all") return logs.value.length
  return logs.value.filter((item) => item.action === key).length
}

async function refresh() {
  loading.value = true
  try {
    logs.value = await api.listAuditLogs(500)
  } catch (err) {
    ElMessage.error(`读取操作日志失败：${err}`)
  } finally {
    loading.value = false
  }
}

onMounted(async () => {
  await refresh()
  try {
    ip.value = await api.currentIp()
  } catch {
    ip.value = ""
  }
})
</script>

<template>
  <section class="audit-page">
    <header class="audit-head">
      <button class="back-btn" title="返回工作台" @click="emit('close')">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
          <path
            d="M15 5l-7 7 7 7"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        返回
      </button>
      <div class="head-text">
        <h2>操作日志</h2>
        <p>
          当前身份：{{ identity }}
          <template v-if="ip"> · 本机 IP {{ ip }}</template>
        </p>
      </div>
      <button class="refresh-btn" :disabled="loading" @click="refresh">
        <el-icon><Refresh /></el-icon>
        {{ loading ? "加载中" : "刷新" }}
      </button>
    </header>

    <div class="audit-filters">
      <button
        v-for="item in filters"
        :key="item.key"
        class="chip"
        :class="{ active: filter === item.key }"
        @click="filter = item.key"
      >
        {{ item.label }}
        <span class="chip-count">{{ countOf(item.key) }}</span>
      </button>
      <span class="filters-hint">仅显示当前身份最近的 500 条记录</span>
    </div>

    <div class="audit-table">
      <div class="thead">
        <span class="col-time">时间</span>
        <span class="col-type">类型</span>
        <span class="col-target">目标 / 内容</span>
        <span class="col-ip">IP</span>
      </div>

      <div class="tbody">
        <div v-if="!loading && filtered.length === 0" class="empty">
          <span class="empty-mark">&gt;_</span>
          <p>暂无操作记录</p>
        </div>

        <div v-for="item in filtered" :key="item.id" class="trow">
          <span class="col-time mono">{{ item.createdAt }}</span>
          <span class="col-type">
            <span class="tag" :class="meta[item.action]?.cls || 'command'">
              {{ meta[item.action]?.label || item.action }}
            </span>
          </span>
          <span class="col-target" :title="item.target">{{ item.target || "—" }}</span>
          <span class="col-ip mono">{{ item.ip || "—" }}</span>
        </div>
      </div>
    </div>
  </section>
</template>

<style scoped>
.audit-page {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: flex;
  flex-direction: column;
  background: radial-gradient(900px 420px at 24% -12%, rgba(45, 212, 191, 0.06), transparent 62%),
    var(--bg-0);
  animation: auditIn 0.22s ease both;
}

@keyframes auditIn {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.audit-head {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 14px;
  padding: 20px 26px 14px;
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

.audit-filters {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 0 26px 12px;
  flex-wrap: wrap;
}

.chip {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  height: 28px;
  padding: 0 11px;
  border-radius: 999px;
  border: 1px solid #22323c;
  background: #0c151b;
  color: #94aab6;
  font-size: 12px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.chip:hover {
  border-color: #2f4653;
  color: #cfe3ec;
}

.chip.active {
  border-color: rgba(45, 212, 191, 0.45);
  background: rgba(45, 212, 191, 0.1);
  color: #85f5e7;
}

.chip-count {
  font-family: var(--font-mono);
  font-size: 10.5px;
  color: #5c7580;
}

.chip.active .chip-count {
  color: #4ecdc0;
}

.filters-hint {
  margin-left: auto;
  font-size: 11px;
  color: #4d6470;
}

.audit-table {
  flex: 1;
  min-height: 0;
  display: flex;
  flex-direction: column;
  margin: 0 26px 22px;
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0d171d, #0a1116);
  overflow: hidden;
}

.thead,
.trow {
  display: grid;
  grid-template-columns: 160px 82px minmax(0, 1fr) 128px;
  align-items: center;
  gap: 10px;
  padding: 0 16px;
}

.thead {
  flex: 0 0 38px;
  font-size: 11px;
  letter-spacing: 0.4px;
  color: #5e7a87;
  border-bottom: 1px solid #17252d;
  background: #0b141a;
}

.tbody {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
}

.trow {
  min-height: 40px;
  border-bottom: 1px solid #121d24;
  font-size: 12.5px;
  color: #c4d5df;
}

.trow:hover {
  background: #101b22;
}

.mono {
  font-family: var(--font-mono);
  font-size: 11.5px;
  color: #8ba0ad;
}

.col-target {
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.col-ip {
  color: #6d8896;
}

.tag {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 5px;
  font-size: 11px;
  border: 1px solid transparent;
  white-space: nowrap;
}

.tag.login {
  color: #7ee7c8;
  background: rgba(52, 211, 153, 0.1);
  border-color: rgba(52, 211, 153, 0.3);
}

.tag.logout {
  color: #9fb3c0;
  background: rgba(148, 163, 184, 0.1);
  border-color: rgba(148, 163, 184, 0.28);
}

.tag.connect {
  color: #9cc8ff;
  background: rgba(96, 165, 250, 0.1);
  border-color: rgba(96, 165, 250, 0.3);
}

.tag.command {
  color: #f8d190;
  background: rgba(251, 191, 36, 0.1);
  border-color: rgba(251, 191, 36, 0.28);
}

.empty {
  padding: 64px 20px;
  text-align: center;
  color: var(--text-mute);
}

.empty-mark {
  display: block;
  font-family: var(--font-mono);
  font-size: 20px;
  color: #33474f;
  margin-bottom: 10px;
}

.empty p {
  margin: 0;
  font-size: 12px;
}
</style>
