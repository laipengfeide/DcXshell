<script setup lang="ts">
import { computed, onBeforeUnmount, ref, watch } from "vue"
import { open, save } from "@tauri-apps/plugin-dialog"
import { listen, type UnlistenFn } from "@tauri-apps/api/event"
import { ElMessage, ElMessageBox } from "element-plus"
import { Delete, CopyDocument, Document, DocumentCopy, EditPen, Folder, FolderAdd, FolderOpened, Refresh, Top, Upload } from "@element-plus/icons-vue"
import * as api from "@/api"
import type { DirListing, RemoteEntry, TransferEvent } from "@/types"

const props = defineProps<{
  modelValue: boolean
  clientKey: string
  host: string
  username: string
}>()

const emit = defineEmits<{
  "update:modelValue": [value: boolean]
}>()

const visible = computed({
  get: () => props.modelValue,
  set: (value) => emit("update:modelValue", value),
})

const currentPath = ref("/home")
const entries = ref<RemoteEntry[]>([])
const commonDirs = ref<string[]>([])
const loading = ref(false)
const errorMsg = ref("")
const selected = ref<RemoteEntry | null>(null)
const busy = ref<"" | "upload" | "download" | "copy" | "extract">("")
const transferName = ref("")
const percent = ref(-1)
const transferred = ref(0)
const totalBytes = ref(0)
const menu = ref<{ x: number; y: number; entry: RemoteEntry } | null>(null)
const clip = ref<RemoteEntry | null>(null)

let unTransfer: UnlistenFn | null = null

const crumbs = computed(() => {
  const parts = currentPath.value.split("/").filter(Boolean)
  const list: { label: string; path: string }[] = [{ label: "/", path: "/" }]
  let acc = ""
  for (const part of parts) {
    acc += `/${part}`
    list.push({ label: part, path: acc })
  }
  return list
})

const parentPath = computed(() => {
  const trimmed = currentPath.value.replace(/\/+$/, "")
  const pos = trimmed.lastIndexOf("/")
  return pos === 0 ? "/" : trimmed.slice(0, pos) || "/"
})

function fileBase(p: string): string {
  const parts = p.split(/[\\/]/)
  return parts[parts.length - 1] || p
}

function humanSize(size: number): string {
  if (size <= 0) return "0 B"
  const units = ["B", "KB", "MB", "GB", "TB"]
  let value = size
  let i = 0
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024
    i += 1
  }
  return `${value < 10 && i > 0 ? value.toFixed(1) : Math.round(value)} ${units[i]}`
}

function formatTime(ts: number | null): string {
  if (!ts) return "—"
  const d = new Date(ts * 1000)
  const pad = (n: number) => String(n).padStart(2, "0")
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(
    d.getMinutes()
  )}`
}

async function load(target?: string) {
  if (!props.clientKey) return
  loading.value = true
  errorMsg.value = ""
  selected.value = null
  try {
    const listing: DirListing = await api.sftpList(props.clientKey, target ?? currentPath.value)
    currentPath.value = listing.path || target || currentPath.value
    entries.value = listing.entries
    if (listing.commonDirs?.length) commonDirs.value = listing.commonDirs
  } catch (err) {
    errorMsg.value = String(err)
    entries.value = []
  } finally {
    loading.value = false
  }
}

function enterDir(entry: RemoteEntry) {
  if (!entry.isDir) return
  void load(entry.path)
}

function onRowDblClick(row: RemoteEntry) {
  if (row.isDir) enterDir(row)
}

async function uploadFiles() {
  if (busy.value) return
  const picked = await open({
    multiple: true,
    directory: false,
    title: `选择要上传到 ${currentPath.value} 的文件`,
  })
  if (!picked) return
  const paths = Array.isArray(picked) ? picked : [picked]
  if (!paths.length) return

  busy.value = "upload"
  percent.value = 0
  transferred.value = 0
  totalBytes.value = 0
  transferName.value = paths.length > 1 ? `${paths.length} 个文件` : fileBase(paths[0])
  try {
    const results = await api.sftpUpload(props.clientKey, paths, currentPath.value)
    ElMessage.success(
      results.length > 1
        ? `已上传 ${results.length} 个文件到 ${currentPath.value}`
        : `已上传 ${results[0].name} 到 ${currentPath.value}`
    )
    await load()
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    busy.value = ""
    percent.value = -1
  }
}

async function downloadSelected() {
  const entry = selected.value
  if (!entry || entry.isDir || busy.value) return
  const target = await save({ defaultPath: entry.name, title: "选择下载保存位置" })
  if (!target) return

  busy.value = "download"
  percent.value = 0
  transferred.value = 0
  totalBytes.value = 0
  transferName.value = entry.name
  try {
    const result = await api.sftpDownload(props.clientKey, entry.path, target)
    ElMessage.success(`已下载 ${result.name} 到本地`)
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    busy.value = ""
    percent.value = -1
  }
}

function onRowContextMenu(row: RemoteEntry, _column: unknown, e: MouseEvent) {
  e.preventDefault()
  selected.value = row
  menu.value = { x: e.clientX, y: e.clientY, entry: row }
}

function closeMenu() {
  menu.value = null
}

async function renameEntry() {
  const entry = menu.value?.entry
  closeMenu()
  if (!entry || busy.value) return
  let newName = ""
  try {
    const result = await ElMessageBox.prompt(`将「${entry.name}」重命名为：`, "重命名", {
      inputValue: entry.name,
      confirmButtonText: "确定",
      cancelButtonText: "取消",
      inputValidator: (v: string) => {
        const n = (v ?? "").trim()
        if (!n) return "名称不能为空"
        if (n.includes("/") || n.includes("\\")) return "名称不能包含路径分隔符"
        if (n === entry.name) return "新名称与原名称相同"
        return true
      },
    })
    newName = (result.value ?? "").trim()
  } catch {
    return
  }
  try {
    const r = await api.sftpRename(props.clientKey, entry.path, newName)
    ElMessage.success(`已重命名为 ${r.name}`)
    await load()
  } catch (err) {
    ElMessage.error(String(err))
  }
}

async function removeEntry() {
  const entry = menu.value?.entry
  closeMenu()
  if (!entry || busy.value) return
  try {
    await ElMessageBox.confirm(
      entry.isDir
        ? `确定删除目录「${entry.name}」吗？仅支持删除空目录。`
        : `确定删除文件「${entry.name}」吗？删除后无法恢复。`,
      "删除确认",
      { confirmButtonText: "删除", cancelButtonText: "取消", type: "warning" }
    )
  } catch {
    return
  }
  try {
    await api.sftpRemove(props.clientKey, entry.path, entry.isDir)
    ElMessage.success(`已删除 ${entry.name}`)
    await load()
  } catch (err) {
    ElMessage.error(String(err))
  }
}

function copyEntry() {
  const entry = menu.value?.entry
  closeMenu()
  if (!entry) return
  if (entry.isDir) {
    ElMessage.warning("暂不支持复制目录，请逐个复制目录中的文件")
    return
  }
  clip.value = { ...entry }
  ElMessage.success(`已复制 ${entry.name}，切换到目标目录后右键或点「粘贴」`)
}

async function pasteEntry() {
  closeMenu()
  const source = clip.value
  if (!source || busy.value) return

  const sameDir = joinPath(currentPath.value, source.name) === source.path
  const duplicate = sameDir || entries.value.some((e) => e.name === source.name)
  let policy: "overwrite" | "rename" = "overwrite"
  if (duplicate) {
    try {
      await ElMessageBox.confirm(
        `当前目录已存在同名文件「${source.name}」。\n「覆盖」将替换原文件；「重命名新文件」将保存为 ${nextName(
          source.name
        )}；点右上角关闭或按 Esc 取消。`,
        "同名文件处理",
        {
          confirmButtonText: "覆盖",
          cancelButtonText: "重命名新文件",
          distinguishCancelAndClose: true,
          type: "warning",
        }
      )
      if (sameDir) {
        ElMessage.warning("同一目录内不能覆盖自身，请改用「重命名新文件」")
        return
      }
    } catch (action) {
      if (action === "cancel") {
        policy = "rename"
      } else {
        return
      }
    }
  }

  busy.value = "copy"
  percent.value = 0
  transferred.value = 0
  totalBytes.value = source.size
  transferName.value = source.name
  try {
    const result = await api.sftpCopy(props.clientKey, source.path, currentPath.value, policy)
    ElMessage.success(`已粘贴为 ${result.name}`)
    await load()
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    busy.value = ""
    percent.value = -1
  }
}

function joinPath(dir: string, name: string): string {
  const base = dir.replace(/\/+$/, "")
  return base ? `${base}/${name}` : `/${name}`
}

function nextName(name: string): string {
  const pos = name.lastIndexOf(".")
  if (pos <= 0) return `${name}(2)`
  return `${name.slice(0, pos)}(2)${name.slice(pos)}`
}

const ARCHIVE_SUFFIXES = [".tar.gz", ".tar.bz2", ".tar.xz", ".tgz", ".tbz2", ".tar", ".zip", ".gz", ".bz2", ".xz"]

function isArchive(name: string): boolean {
  const lower = name.toLowerCase()
  return ARCHIVE_SUFFIXES.some((suffix) => lower.endsWith(suffix))
}

function archiveStem(name: string): string {
  const lower = name.toLowerCase()
  const suffix = ARCHIVE_SUFFIXES.find((item) => lower.endsWith(item) && name.length > item.length)
  return suffix ? name.slice(0, name.length - suffix.length) : name
}

async function extractEntry(mode: "here" | "folder") {
  const entry = menu.value?.entry
  closeMenu()
  if (!entry || busy.value) return

  if (mode === "folder") {
    const folder = archiveStem(entry.name)
    if (entries.value.some((item) => item.isDir && item.name === folder)) {
      ElMessage.warning(`同名文件夹「${folder}」已存在，请先删除或重命名后再解压`)
      return
    }
  } else {
    try {
      await ElMessageBox.confirm(
        `将「${entry.name}」解压到当前目录 ${currentPath.value}，如遇同名文件会直接覆盖，是否继续？`,
        "解压到当前文件夹",
        { confirmButtonText: "开始解压", cancelButtonText: "取消", type: "warning" }
      )
    } catch {
      return
    }
  }

  busy.value = "extract"
  percent.value = -1
  transferred.value = 0
  totalBytes.value = 0
  transferName.value = entry.name
  try {
    const dir = await api.sftpExtract(props.clientKey, entry.path, mode)
    ElMessage.success(`已解压 ${entry.name} 到 ${dir}`)
    await load()
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    busy.value = ""
    percent.value = -1
  }
}

async function bindTransfer() {
  unTransfer?.()
  unTransfer = await listen<TransferEvent>("dcx:transfer", (event) => {
    const payload = event.payload
    if (payload.clientKey !== props.clientKey) return
    if (busy.value && payload.direction !== busy.value) return
    transferName.value = payload.name
    transferred.value = payload.transferred
    totalBytes.value = payload.total
    if (payload.done) {
      percent.value = 100
      return
    }
    if (payload.total > 0) {
      percent.value = Math.min(99, Math.round((payload.transferred / payload.total) * 100))
    } else {
      percent.value = -1
    }
  })
}

watch(
  () => props.modelValue,
  async (open) => {
    if (open) {
      if (!unTransfer) await bindTransfer()
      document.addEventListener("click", closeMenu)
      await load("/home")
    } else {
      unTransfer?.()
      unTransfer = null
      document.removeEventListener("click", closeMenu)
      closeMenu()
      entries.value = []
      selected.value = null
      errorMsg.value = ""
    }
  }
)

onBeforeUnmount(() => {
  unTransfer?.()
  document.removeEventListener("click", closeMenu)
})
</script>

<template>
  <el-dialog
    v-model="visible"
    class="ft-dialog"
    width="900px"
    top="6vh"
    :close-on-click-modal="false"
    :show-close="true"
  >
    <template #header>
      <div class="ft-head">
        <div>
          <h3>文件传输</h3>
          <p>{{ username }}@{{ host }} · SFTP</p>
        </div>
      </div>
    </template>

    <div class="ft-body">
      <aside class="ft-side">
        <h4>常用目录</h4>
        <button
          v-for="dir in commonDirs"
          :key="dir"
          class="dir-item"
          :class="{ active: dir === currentPath }"
          @click="load(dir)"
        >
          <el-icon><FolderOpened /></el-icon>
          <span>{{ dir }}</span>
        </button>
        <p class="side-tip">默认上传目录为 <b>/home</b></p>
      </aside>

      <main class="ft-main">
        <div class="ft-bar">
          <button class="icon-btn" title="上一级" :disabled="currentPath === '/' || loading" @click="load(parentPath)">
            <el-icon><Top /></el-icon>
          </button>
          <div class="crumbs">
            <button
              v-for="(crumb, index) in crumbs"
              :key="crumb.path"
              class="crumb"
              :class="{ active: index === crumbs.length - 1 }"
              @click="load(crumb.path)"
            >
              {{ crumb.label }}
            </button>
          </div>
          <button
            class="icon-btn"
            :title="clip ? `粘贴 ${clip.name} 到 ${currentPath}` : '先在文件上右键复制'"
            :disabled="!clip || loading || !!busy"
            @click="pasteEntry"
          >
            <el-icon><DocumentCopy /></el-icon>
          </button>
          <button class="icon-btn" title="刷新" :disabled="loading" @click="load()">
            <el-icon><Refresh /></el-icon>
          </button>
        </div>

        <div class="ft-table">
          <div v-if="loading" class="ft-tip">正在读取服务器目录…</div>
          <div v-else-if="errorMsg" class="ft-tip err">{{ errorMsg }}</div>
          <el-table
            v-else
            :data="entries"
            height="318"
            highlight-current-row
            size="small"
            @row-click="(row: RemoteEntry) => (selected = row)"
            @row-dblclick="onRowDblClick"
            @row-contextmenu="onRowContextMenu"
          >
            <el-table-column label="名称" min-width="260">
              <template #default="{ row }">
                <span class="entry" :class="{ dir: row.isDir }">
                  <el-icon>
                    <Folder v-if="row.isDir" />
                    <Document v-else />
                  </el-icon>
                  <span class="entry-name">{{ row.name }}</span>
                  <span v-if="row.isLink" class="tag">链接</span>
                </span>
              </template>
            </el-table-column>
            <el-table-column label="大小" width="110" align="right">
              <template #default="{ row }">{{ row.isDir ? "—" : humanSize(row.size) }}</template>
            </el-table-column>
            <el-table-column label="修改时间" width="160">
              <template #default="{ row }">{{ formatTime(row.modified) }}</template>
            </el-table-column>
          </el-table>
        </div>

        <div v-if="busy" class="ft-mask">
          <div class="ft-transfer">
            <div class="t-title">
              {{ busy === "upload" ? "正在上传" : busy === "copy" ? "正在复制" : busy === "extract" ? "正在解压" : "正在下载" }}
            </div>
            <div class="t-name" :title="transferName">{{ transferName }}</div>
            <el-progress
              :percentage="percent < 0 ? 0 : percent"
              :indeterminate="percent < 0"
              :stroke-width="10"
              striped
              striped-flow
            />
            <div class="t-size">
              <span v-if="totalBytes > 0">{{ humanSize(transferred) }} / {{ humanSize(totalBytes) }}</span>
              <span v-else>{{ humanSize(transferred) }}</span>
              <b v-if="percent >= 0">{{ percent }}%</b>
            </div>
          </div>
        </div>

        <div class="ft-actions">
          <el-button type="primary" :icon="Upload" :disabled="!!busy" @click="uploadFiles">
            上传文件到 {{ currentPath }}
          </el-button>
          <el-button :disabled="!selected || selected.isDir || !!busy" @click="downloadSelected">
            下载选中文件
          </el-button>
          <span class="hint">双击目录进入，右键文件可复制 / 粘贴 / 解压 / 重命名 / 删除</span>
        </div>
      </main>
    </div>

    <teleport to="body">
      <div
        v-if="menu"
        class="ft-menu"
        :style="{ left: menu.x + 'px', top: menu.y + 'px' }"
        @click.stop
      >
        <button @click="copyEntry">
          <el-icon><CopyDocument /></el-icon>
          <span>复制</span>
        </button>
        <button :disabled="!clip" @click="pasteEntry">
          <el-icon><DocumentCopy /></el-icon>
          <span>粘贴{{ clip ? ` ${clip.name}` : "" }}</span>
        </button>
        <button v-if="isArchive(menu.entry.name)" @click="extractEntry('here')">
          <el-icon><FolderOpened /></el-icon>
          <span>解压到当前文件夹</span>
        </button>
        <button v-if="isArchive(menu.entry.name)" @click="extractEntry('folder')">
          <el-icon><FolderAdd /></el-icon>
          <span>解压到同名文件夹</span>
        </button>
        <button @click="renameEntry">
          <el-icon><EditPen /></el-icon>
          <span>重命名</span>
        </button>
        <button class="danger" @click="removeEntry">
          <el-icon><Delete /></el-icon>
          <span>删除</span>
        </button>
      </div>
    </teleport>
  </el-dialog>
</template>

<style scoped>
.ft-head h3 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: var(--text);
}

.ft-head p {
  margin: 5px 0 0;
  font-size: 12px;
  color: var(--text-mute);
}

.ft-body {
  display: grid;
  grid-template-columns: 208px 1fr;
  gap: 14px;
  padding-bottom: 4px;
}

.ft-side {
  border: 1px solid var(--border);
  border-radius: 10px;
  background: #0b131a;
  padding: 12px 10px;
  display: flex;
  flex-direction: column;
  min-height: 420px;
}

.ft-side h4 {
  margin: 2px 0 10px 4px;
  font-size: 12px;
  font-weight: 600;
  color: var(--text-dim);
  letter-spacing: 0.4px;
}

.dir-item {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 8px 9px;
  border: 1px solid transparent;
  border-radius: 8px;
  background: transparent;
  color: var(--text-dim);
  font-size: 12.5px;
  text-align: left;
  cursor: pointer;
  overflow: hidden;
}

.dir-item span {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.dir-item:hover {
  background: #10202a;
  color: var(--text);
}

.dir-item.active {
  border-color: rgba(45, 212, 191, 0.45);
  background: rgba(45, 212, 191, 0.1);
  color: #7ff0e3;
}

.side-tip {
  margin: auto 4px 0;
  padding-top: 10px;
  border-top: 1px dashed #1b2831;
  font-size: 11.5px;
  line-height: 1.7;
  color: var(--text-mute);
}

.side-tip b {
  color: #7ff0e3;
}

.ft-main {
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-width: 0;
}

.ft-bar {
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 9px;
  background: #0b131a;
}

.icon-btn {
  display: grid;
  place-items: center;
  width: 26px;
  height: 26px;
  border: 1px solid transparent;
  border-radius: 7px;
  background: transparent;
  color: var(--text-dim);
  cursor: pointer;
}

.icon-btn:hover:not(:disabled) {
  background: #16242d;
  color: var(--text);
}

.icon-btn:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.crumbs {
  flex: 1;
  min-width: 0;
  display: flex;
  align-items: center;
  flex-wrap: wrap;
  gap: 2px;
  font-size: 12.5px;
}

.crumb {
  padding: 2px 5px;
  border: 0;
  border-radius: 5px;
  background: transparent;
  color: var(--text-mute);
  cursor: pointer;
}

.crumb:hover {
  color: var(--text);
  background: #16242d;
}

.crumb.active {
  color: #7ff0e3;
}

.ft-table {
  position: relative;
  border: 1px solid var(--border);
  border-radius: 10px;
  overflow: hidden;
  background: #0a1218;
}

.ft-tip {
  display: grid;
  place-items: center;
  height: 318px;
  font-size: 12.5px;
  color: var(--text-mute);
}

.ft-tip.err {
  color: #fca5a5;
  padding: 0 24px;
  text-align: center;
  line-height: 1.7;
}

.entry {
  display: inline-flex;
  align-items: center;
  gap: 7px;
  min-width: 0;
  color: var(--text-dim);
}

.entry.dir {
  color: #8ee9df;
}

.entry-name {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.tag {
  padding: 0 5px;
  border-radius: 4px;
  border: 1px solid #2c3d49;
  color: var(--text-mute);
  font-size: 10px;
}

.ft-mask {
  position: absolute;
  inset: 0;
  z-index: 10;
  display: grid;
  place-items: center;
  border-radius: 10px;
  background: rgba(5, 10, 14, 0.72);
  backdrop-filter: blur(2px);
}

.ft-transfer {
  width: 340px;
  padding: 18px 20px 16px;
  border: 1px solid rgba(45, 212, 191, 0.35);
  border-radius: 12px;
  background: #0b151c;
  box-shadow: 0 14px 42px rgba(0, 0, 0, 0.5);
}

.t-title {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
  letter-spacing: 0.4px;
}

.t-name {
  margin: 6px 0 10px;
  font-size: 12.5px;
  color: #7ff0e3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.t-size {
  display: flex;
  justify-content: space-between;
  margin-top: 8px;
  font-size: 11.5px;
  color: var(--text-dim);
}

.t-size b {
  color: #7ff0e3;
}

.ft-menu {
  position: fixed;
  z-index: 4000;
  min-width: 120px;
  padding: 5px;
  border: 1px solid var(--border);
  border-radius: 9px;
  background: #0c161d;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.5);
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.ft-menu button {
  display: flex;
  align-items: center;
  gap: 8px;
  width: 100%;
  padding: 7px 12px;
  border: 0;
  border-radius: 6px;
  background: transparent;
  color: var(--text-dim);
  font-size: 12.5px;
  cursor: pointer;
}

.ft-menu button:hover {
  background: #16242d;
  color: var(--text);
}

.ft-menu button:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}

.ft-menu button.danger:hover {
  background: rgba(248, 113, 113, 0.14);
  color: #fca5a5;
}

.ft-actions {
  display: flex;
  align-items: center;
  gap: 10px;
}

.ft-actions .hint {
  margin-left: auto;
  font-size: 11.5px;
  color: var(--text-mute);
}
</style>
