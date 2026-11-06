<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, reactive, ref } from "vue"
import { Search, Plus, Refresh } from "@element-plus/icons-vue"
import { ElTree } from "element-plus"
import type { ConnectionItem, SessionStatus, UserInfo } from "@/types"

interface TreeNode {
  id: string
  label: string
  isGroup: boolean
  children?: TreeNode[]
  conn?: ConnectionItem
}

const props = defineProps<{
  connections: ConnectionItem[]
  statusByConnection: Record<number, SessionStatus>
  openedCount: number
  user: UserInfo | null
}>()

const emit = defineEmits<{
  open: [ConnectionItem]
  new: []
  edit: [ConnectionItem]
  remove: [ConnectionItem]
  refresh: []
  loginRequest: []
  logout: []
}>()

const keyword = ref("")
const treeRef = ref<InstanceType<typeof ElTree> | null>(null)

const filtered = computed(() => {
  const q = keyword.value.trim().toLowerCase()
  if (!q) return props.connections
  return props.connections.filter(
    (c) =>
      c.name.toLowerCase().includes(q) ||
      c.host.toLowerCase().includes(q) ||
      c.username.toLowerCase().includes(q) ||
      c.groupName.toLowerCase().includes(q)
  )
})

const treeData = computed<TreeNode[]>(() => {
  const groups = new Map<string, TreeNode[]>()
  for (const conn of filtered.value) {
    const groupName = conn.groupName || "默认分组"
    if (!groups.has(groupName)) groups.set(groupName, [])
    groups.get(groupName)!.push({
      id: `conn-${conn.id}`,
      label: conn.name,
      isGroup: false,
      conn,
    })
  }
  return Array.from(groups.entries()).map(([name, children]) => ({
    id: `group-${name}`,
    label: name,
    isGroup: true,
    children,
  }))
})

const menu = reactive({
  visible: false,
  x: 0,
  y: 0,
  conn: null as ConnectionItem | null,
})

function onNodeClick(node: TreeNode) {
  if (node.isGroup) {
    const treeNode = treeRef.value?.getNode(node.id as any)
    if (!treeNode) return
    treeNode.expanded ? treeNode.collapse() : treeNode.expand()
    return
  }
  if (node.conn) emit("open", node.conn)
}

function openMenu(event: MouseEvent, conn: ConnectionItem) {
  event.preventDefault()
  event.stopPropagation()
  menu.conn = conn
  menu.visible = true
  menu.x = Math.min(event.clientX, window.innerWidth - 170)
  menu.y = Math.min(event.clientY, window.innerHeight - 130)
}

function closeMenu() {
  menu.visible = false
}

function runMenu(action: "open" | "edit" | "remove") {
  const conn = menu.conn
  closeMenu()
  if (!conn) return
  if (action === "open") emit("open", conn)
  if (action === "edit") emit("edit", conn)
  if (action === "remove") emit("remove", conn)
}

function dotClass(connId: number) {
  const status = props.statusByConnection[connId]
  return status ? `dot-${status}` : "dot-idle"
}

const initials = computed(() =>
  props.user ? props.user.username.slice(0, 1).toUpperCase() : "G"
)

onMounted(() => document.addEventListener("click", closeMenu))
onBeforeUnmount(() => document.removeEventListener("click", closeMenu))
</script>

<template>
  <aside class="sidebar">
    <div class="side-top">
      <div class="side-title">
        <span>连接</span>
        <span class="count">{{ connections.length }}</span>
        <button class="icon-btn" title="刷新连接列表" @click="emit('refresh')">
          <el-icon><Refresh /></el-icon>
        </button>
      </div>

      <el-input
        v-model="keyword"
        class="search"
        size="default"
        placeholder="搜索名称 / 主机 / 用户"
        :prefix-icon="Search"
        clearable
      />

      <button class="new-btn" @click="emit('new')">
        <el-icon><Plus /></el-icon>
        新建连接
      </button>
    </div>

    <div class="side-tree">
      <div v-if="filtered.length === 0" class="empty">
        <div class="empty-icon">&gt;_</div>
        <p v-if="keyword">没有匹配的连接</p>
        <p v-else>还没有连接配置</p>
        <button v-if="!keyword" class="empty-btn" @click="emit('new')">创建第一个连接</button>
      </div>

      <el-tree
        v-else
        ref="treeRef"
        :data="treeData"
        node-key="id"
        :props="{ label: 'label', children: 'children' }"
        default-expand-all
        :expand-on-click-node="false"
        :highlight-current="false"
        @node-click="onNodeClick"
      >
        <template #default="{ data }">
          <div v-if="data.isGroup" class="group-node">
            <span class="group-name">{{ data.label }}</span>
            <span class="group-count">{{ data.children?.length ?? 0 }}</span>
          </div>
          <div
            v-else
            class="conn-node"
            @contextmenu="openMenu($event, data.conn)"
            @dblclick.stop="emit('open', data.conn)"
          >
            <span class="dot" :class="dotClass(data.conn.id)"></span>
            <div class="conn-text">
              <span class="conn-name">{{ data.conn.name }}</span>
              <span class="conn-host">{{ data.conn.username }}@{{ data.conn.host }}</span>
            </div>
          </div>
        </template>
      </el-tree>
    </div>

    <div class="side-foot">
      <div class="avatar" :class="{ guest: !user }">{{ initials }}</div>
      <div class="who">
        <div class="uname">{{ user ? user.username : "访客模式" }}</div>
        <div class="umeta">
          <span v-if="user" class="unlimited">已登录 · 无限窗口</span>
          <span v-else>并行窗口 {{ openedCount }} / 3</span>
        </div>
      </div>
      <button v-if="user" class="acct-btn" @click="emit('logout')">退出</button>
      <button v-else class="acct-btn accent" @click="emit('loginRequest')">登录</button>
    </div>

    <div
      v-if="menu.visible"
      class="ctx-menu"
      :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
      @click.stop
    >
      <button @click="runMenu('open')">打开连接</button>
      <button @click="runMenu('edit')">编辑配置</button>
      <button class="danger" @click="runMenu('remove')">删除连接</button>
    </div>
  </aside>
</template>

<style scoped>
.sidebar {
  width: 278px;
  flex: 0 0 278px;
  display: flex;
  flex-direction: column;
  background: linear-gradient(180deg, #0b1318, #080e12);
  border-right: 1px solid var(--border);
  min-height: 0;
  position: relative;
}

.side-top {
  padding: 14px 14px 10px;
}

.side-title {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 11px;
  color: var(--text-dim);
  font-size: 12px;
  letter-spacing: 0.6px;
  text-transform: uppercase;
}

.count {
  padding: 1px 6px;
  border-radius: 6px;
  background: #132029;
  color: var(--text-mute);
  font-size: 10px;
  letter-spacing: 0;
}

.icon-btn {
  margin-left: auto;
  width: 24px;
  height: 24px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-mute);
  cursor: pointer;
  font-size: 13px;
}

.icon-btn:hover {
  background: #15222a;
  color: var(--text-dim);
}

.search {
  margin-bottom: 9px;
}

.new-btn {
  width: 100%;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  border-radius: 9px;
  border: 1px solid rgba(45, 212, 191, 0.32);
  background: linear-gradient(180deg, rgba(45, 212, 191, 0.13), rgba(45, 212, 191, 0.05));
  color: #7ff2e2;
  font-size: 13px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.18s ease;
}

.new-btn:hover {
  border-color: rgba(45, 212, 191, 0.6);
  background: rgba(45, 212, 191, 0.16);
  color: #d6fff9;
  box-shadow: 0 0 22px rgba(45, 212, 191, 0.12);
}

.side-tree {
  flex: 1;
  overflow-y: auto;
  overflow-x: hidden;
  padding: 2px 6px 10px;
  min-height: 0;
}

.empty {
  padding: 42px 16px;
  text-align: center;
  color: var(--text-mute);
}

.empty-icon {
  width: 46px;
  height: 46px;
  margin: 0 auto 12px;
  display: grid;
  place-items: center;
  border-radius: 12px;
  border: 1px dashed #253641;
  color: #415966;
  font-family: var(--font-mono);
  font-size: 15px;
}

.empty p {
  font-size: 12px;
  margin: 0 0 12px;
}

.empty-btn {
  height: 30px;
  padding: 0 14px;
  border-radius: 7px;
  border: 1px solid #263842;
  background: transparent;
  color: var(--text-dim);
  font-size: 12px;
  cursor: pointer;
}

.empty-btn:hover {
  border-color: rgba(45, 212, 191, 0.45);
  color: var(--accent);
}

.group-node {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 2px 0;
  font-size: 12px;
  color: #7e95a2;
}

.group-name {
  font-weight: 600;
  letter-spacing: 0.2px;
}

.group-count {
  margin-left: auto;
  font-size: 10px;
  color: #4d6470;
}

.conn-node {
  display: flex;
  align-items: center;
  gap: 9px;
  width: 100%;
  padding: 4px 6px 4px 2px;
  border-radius: 7px;
  overflow: hidden;
}

.conn-node:hover {
  background: #121e26;
}

.dot {
  width: 7px;
  height: 7px;
  flex: 0 0 7px;
  border-radius: 50%;
  background: #31434d;
}

.dot-online {
  background: var(--green);
  box-shadow: 0 0 8px rgba(52, 211, 153, 0.7);
}

.dot-reconnecting {
  background: var(--amber);
  box-shadow: 0 0 8px rgba(251, 191, 36, 0.65);
  animation: blink 1s ease-in-out infinite;
}

.dot-offline {
  background: var(--red);
  box-shadow: 0 0 8px rgba(248, 113, 113, 0.55);
}

@keyframes blink {
  50% {
    opacity: 0.35;
  }
}

.conn-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
  line-height: 1.35;
}

.conn-name {
  font-size: 12.5px;
  color: #ccdde6;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.conn-host {
  font-size: 10.5px;
  color: #536975;
  font-family: var(--font-mono);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.side-foot {
  flex: 0 0 auto;
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 11px 12px;
  border-top: 1px solid var(--border);
  background: #0a1116;
}

.avatar {
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  background: linear-gradient(135deg, #0e7490, #0f766e);
  color: #e9fffb;
  font-size: 12px;
  font-weight: 700;
}

.avatar.guest {
  background: linear-gradient(135deg, #334155, #1e293b);
  color: #9fb3c0;
}

.who {
  min-width: 0;
  flex: 1;
}

.uname {
  font-size: 12.5px;
  color: #d3e4ec;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.umeta {
  font-size: 10.5px;
  color: var(--text-mute);
  margin-top: 2px;
}

.unlimited {
  color: #4ecdc0;
}

.acct-btn {
  height: 28px;
  padding: 0 10px;
  border-radius: 7px;
  border: 1px solid #24343e;
  background: transparent;
  color: var(--text-dim);
  font-size: 11.5px;
  cursor: pointer;
}

.acct-btn:hover {
  border-color: #33495477;
  background: #101a20;
}

.acct-btn.accent {
  border-color: rgba(45, 212, 191, 0.4);
  color: var(--accent);
  background: rgba(45, 212, 191, 0.07);
}

.ctx-menu {
  position: fixed;
  z-index: 9999;
  min-width: 148px;
  padding: 4px;
  border-radius: 9px;
  background: #101b22;
  border: 1px solid #263842;
  box-shadow: 0 16px 40px rgba(0, 0, 0, 0.55);
}

.ctx-menu button {
  display: block;
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

.ctx-menu button:hover {
  background: #17262f;
  color: #e6f4fb;
}

.ctx-menu button.danger:hover {
  background: rgba(248, 113, 113, 0.12);
  color: #ff9c9c;
}

/* el-tree 细节 */
.side-tree :deep(.el-tree) {
  background: transparent;
  color: var(--text-dim);
}

.side-tree :deep(.el-tree-node__content) {
  height: 36px;
  border-radius: 7px;
}

.side-tree :deep(.el-tree-node__content:hover) {
  background: transparent;
}

.side-tree :deep(.el-tree-node__expand-icon) {
  color: #526874;
  padding: 4px;
}

.side-tree :deep(.el-tree-node__expand-icon.is-leaf) {
  display: none;
}

.side-tree :deep(.el-tree-node:focus > .el-tree-node__content) {
  background: transparent;
}
</style>
