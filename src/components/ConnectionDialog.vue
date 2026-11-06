<script setup lang="ts">
import { computed, reactive, ref, watch } from "vue"
import { ElMessage } from "element-plus"
import { open } from "@tauri-apps/plugin-dialog"
import * as api from "@/api"
import type { ConnectionInput, ConnectionItem, TestResult } from "@/types"

const props = defineProps<{
  visible: boolean
  editing: ConnectionItem | null
}>()

const emit = defineEmits<{
  "update:visible": [boolean]
  saved: []
}>()

const form = reactive({
  name: "",
  groupName: "默认分组",
  host: "",
  port: 22,
  username: "",
  authType: "password" as "password" | "key",
  password: "",
  keyPath: "",
  passphrase: "",
})

const editingId = ref<number | null>(null)
const editingHasPassword = ref(false)
const testing = ref(false)
const saving = ref(false)
const result = ref<TestResult | null>(null)
const testedFingerprint = ref("")

const title = computed(() => (editingId.value ? "编辑连接" : "新建连接"))
const passwordPlaceholder = computed(() => {
  if (editingHasPassword.value) return "留空则沿用已保存的密码"
  return "服务器登录密码"
})

function fingerprint() {
  return JSON.stringify({
    id: editingId.value,
    host: form.host.trim(),
    port: form.port,
    username: form.username.trim(),
    authType: form.authType,
    password: form.password,
    keyPath: form.keyPath,
    passphrase: form.passphrase,
  })
}

const canSave = computed(
  () => result.value?.ok === true && testedFingerprint.value === fingerprint()
)

watch(
  () => props.visible,
  (visible) => {
    if (!visible) return
    const c = props.editing
    editingId.value = c?.id ?? null
    editingHasPassword.value = !!c?.hasPassword
    form.name = c?.name ?? ""
    form.groupName = c?.groupName ?? "默认分组"
    form.host = c?.host ?? ""
    form.port = c?.port ?? 22
    form.username = c?.username ?? ""
    form.authType = c?.authType ?? "password"
    form.password = ""
    form.keyPath = c?.keyPath ?? ""
    form.passphrase = ""
    result.value = null
    testedFingerprint.value = ""
    testing.value = false
    saving.value = false
  },
  { immediate: true }
)

watch(
  () => [form.host, form.port, form.username, form.authType, form.password, form.keyPath, form.passphrase],
  () => {
    if (result.value) result.value = null
  }
)

function validate(needPassword = false): boolean {
  if (!form.name.trim()) {
    ElMessage.warning("请填写连接名称")
    return false
  }
  if (!form.host.trim()) {
    ElMessage.warning("请填写服务器地址")
    return false
  }
  if (!form.username.trim()) {
    ElMessage.warning("请填写用户名")
    return false
  }
  if (!form.port || form.port < 1 || form.port > 65535) {
    ElMessage.warning("端口需在 1 - 65535 之间")
    return false
  }
  if (form.authType === "key" && !form.keyPath.trim()) {
    ElMessage.warning("私钥登录需要选择私钥文件")
    return false
  }
  if (needPassword && form.authType === "password" && !form.password && !editingHasPassword.value) {
    ElMessage.warning("请输入登录密码")
    return false
  }
  return true
}

function payload(): ConnectionInput {
  return {
    id: editingId.value,
    name: form.name.trim(),
    groupName: form.groupName.trim() || "默认分组",
    host: form.host.trim(),
    port: form.port,
    username: form.username.trim(),
    authType: form.authType,
    password: form.password || undefined,
    keyPath: form.authType === "key" ? form.keyPath.trim() : undefined,
    passphrase: form.authType === "key" ? form.passphrase || undefined : undefined,
  }
}

async function runTest() {
  if (!validate(true)) return
  testing.value = true
  try {
    const snapshot = fingerprint()
    const res = await api.testConnection(payload())
    result.value = res
    testedFingerprint.value = snapshot
    if (res.ok) {
      ElMessage.success(`连接测试通过，耗时 ${res.latencyMs} ms`)
    }
  } catch (err) {
    result.value = { ok: false, message: String(err), latencyMs: 0 }
    testedFingerprint.value = fingerprint()
  } finally {
    testing.value = false
  }
}

async function save() {
  if (!validate(true)) return
  if (!canSave.value) {
    ElMessage.warning("请先点击「测试连接」，测试通过后再保存")
    return
  }
  saving.value = true
  try {
    await api.saveConnection(payload())
    ElMessage.success("连接已保存到左侧连接树")
    emit("saved")
    emit("update:visible", false)
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    saving.value = false
  }
}

async function browseKey() {
  try {
    const picked = await open({
      multiple: false,
      directory: false,
      title: "选择 SSH 私钥文件",
    })
    if (typeof picked === "string") {
      form.keyPath = picked
    }
  } catch (err) {
    ElMessage.error(String(err))
  }
}

function close() {
  emit("update:visible", false)
}
</script>

<template>
  <el-dialog
    :model-value="props.visible"
    class="dc-dialog"
    width="600px"
    top="7vh"
    :show-close="false"
    :close-on-click-modal="false"
    append-to-body
    @update:model-value="close"
  >
    <div class="dlg">
      <header class="dlg-head">
        <div>
          <h3>{{ title }}</h3>
          <p>先测试 SSH 连接，成功后再保存到连接树</p>
        </div>
        <button class="close-btn" @click="close">
          <svg width="12" height="12" viewBox="0 0 12 12">
            <path d="M2 2l8 8M10 2L2 10" stroke="currentColor" stroke-width="1.4" />
          </svg>
        </button>
      </header>

      <div class="dlg-body">
        <div class="row2">
          <div class="field">
            <label>连接名称 <i>*</i></label>
            <el-input v-model="form.name" placeholder="例如：生产 Web 服务器" maxlength="64" />
          </div>
          <div class="field">
            <label>分组</label>
            <el-input v-model="form.groupName" placeholder="默认分组" maxlength="32" />
          </div>
        </div>

        <div class="row-host">
          <div class="field">
            <label>服务器地址 <i>*</i></label>
            <el-input v-model="form.host" placeholder="192.168.1.10 或 example.com" />
          </div>
          <div class="field port">
            <label>端口</label>
            <el-input-number
              v-model="form.port"
              :min="1"
              :max="65535"
              :controls="false"
              style="width: 100%"
            />
          </div>
        </div>

        <div class="field">
          <label>用户名 <i>*</i></label>
          <el-input v-model="form.username" placeholder="登录服务器的用户名" />
        </div>

        <div class="field">
          <label>认证方式</label>
          <el-radio-group v-model="form.authType" class="auth-group">
            <el-radio-button label="password">密码认证</el-radio-button>
            <el-radio-button label="key">私钥认证</el-radio-button>
          </el-radio-group>
        </div>

        <template v-if="form.authType === 'password'">
          <div class="field">
            <label>登录密码</label>
            <el-input
              v-model="form.password"
              type="password"
              show-password
              :placeholder="passwordPlaceholder"
            />
          </div>
        </template>

        <template v-else>
          <div class="field">
            <label>私钥文件 <i>*</i></label>
            <div class="key-row">
              <el-input v-model="form.keyPath" placeholder="选择本地私钥文件（OpenSSH 格式）" readonly />
              <el-button class="browse" @click="browseKey">浏览</el-button>
            </div>
          </div>
          <div class="field">
            <label>私钥口令（可选）</label>
            <el-input
              v-model="form.passphrase"
              type="password"
              show-password
              placeholder="私钥未加密时留空"
            />
          </div>
        </template>

        <div class="test-row">
          <el-button class="test-btn" :loading="testing" @click="runTest">
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" class="plug-icon">
              <path d="M9 3v5M15 3v5M7 8h10v3a5 5 0 01-10 0V8z" stroke="currentColor" stroke-width="1.6" />
              <path d="M12 16v5" stroke="currentColor" stroke-width="1.6" />
            </svg>
            测试连接
          </el-button>
          <span class="test-hint">测试通过后才可保存</span>
        </div>

        <div v-if="result" class="test-result" :class="result.ok ? 'ok' : 'fail'">
          <span class="tr-icon">{{ result.ok ? "✓" : "✕" }}</span>
          <div class="tr-body">
            <strong>{{ result.ok ? "连接成功" : "连接失败" }}</strong>
            <p>{{ result.message }}</p>
          </div>
          <span v-if="result.ok" class="tr-latency">{{ result.latencyMs }} ms</span>
        </div>
      </div>

      <footer class="dlg-foot">
        <el-button @click="close">取消</el-button>
        <el-button type="primary" :disabled="!canSave" :loading="saving" @click="save">
          保存连接
        </el-button>
      </footer>
    </div>
  </el-dialog>
</template>

<style scoped>
.dlg {
  display: flex;
  flex-direction: column;
}

.dlg-head {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  padding: 22px 24px 16px;
  border-bottom: 1px solid #1b2831;
}

.dlg-head h3 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: #eaf6f3;
  letter-spacing: 0.3px;
}

.dlg-head p {
  margin: 6px 0 0;
  font-size: 12px;
  color: var(--text-mute);
}

.close-btn {
  width: 28px;
  height: 28px;
  display: grid;
  place-items: center;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--text-mute);
  cursor: pointer;
}

.close-btn:hover {
  background: #17262f;
  color: var(--text);
}

.dlg-body {
  padding: 18px 24px 6px;
  max-height: 64vh;
  overflow-y: auto;
}

.row2 {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 14px;
}

.row-host {
  display: grid;
  grid-template-columns: 1fr 116px;
  gap: 14px;
}

.field {
  margin-bottom: 14px;
}

.field label {
  display: block;
  margin-bottom: 7px;
  font-size: 12px;
  color: var(--text-dim);
}

.field label i {
  color: #f59e0b;
  font-style: normal;
  margin-left: 2px;
}

.auth-group {
  display: flex;
}

.key-row {
  display: flex;
  gap: 8px;
}

.browse {
  flex: 0 0 auto;
}

.test-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin: 6px 0 4px;
}

.test-btn {
  --el-button-bg-color: #10202a;
  --el-button-border-color: #27404d;
  --el-button-text-color: #9fdcf5;
  --el-button-hover-bg-color: #152a35;
  --el-button-hover-border-color: #3a5d6e;
  --el-button-hover-text-color: #d2f1ff;
}

.plug-icon {
  margin-right: 4px;
}

.test-hint {
  font-size: 11px;
  color: var(--text-mute);
}

.test-result {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  margin: 10px 0 12px;
  padding: 11px 13px;
  border-radius: 9px;
  border: 1px solid transparent;
  font-size: 12px;
}

.test-result.ok {
  background: rgba(52, 211, 153, 0.07);
  border-color: rgba(52, 211, 153, 0.28);
}

.test-result.fail {
  background: rgba(248, 113, 113, 0.07);
  border-color: rgba(248, 113, 113, 0.28);
}

.tr-icon {
  width: 18px;
  height: 18px;
  flex: 0 0 18px;
  display: grid;
  place-items: center;
  border-radius: 50%;
  font-size: 10px;
  font-weight: 700;
  margin-top: 1px;
}

.ok .tr-icon {
  background: rgba(52, 211, 153, 0.16);
  color: #6ee7b7;
}

.fail .tr-icon {
  background: rgba(248, 113, 113, 0.16);
  color: #fca5a5;
}

.tr-body {
  flex: 1;
  min-width: 0;
}

.tr-body strong {
  font-size: 12.5px;
  color: #d9e9f1;
}

.tr-body p {
  margin: 3px 0 0;
  color: var(--text-dim);
  line-height: 1.6;
  word-break: break-all;
}

.tr-latency {
  font-family: var(--font-mono);
  font-size: 11px;
  color: #6ee7b7;
  white-space: nowrap;
  margin-top: 2px;
}

.dlg-foot {
  display: flex;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 24px 20px;
  border-top: 1px solid #1b2831;
}

.dlg-body :deep(.el-input-number .el-input__wrapper) {
  padding-left: 10px;
  padding-right: 10px;
}
</style>
