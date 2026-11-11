<script setup lang="ts">
import { reactive, ref, watch } from "vue"
import { ElMessage } from "element-plus"
import * as api from "@/api"

const props = defineProps<{ visible: boolean }>()
const emit = defineEmits<{
  "update:visible": [boolean]
  changed: []
}>()

const form = reactive({ oldPassword: "", newPassword: "", confirm: "" })
const saving = ref(false)

watch(
  () => props.visible,
  (visible) => {
    if (!visible) return
    form.oldPassword = ""
    form.newPassword = ""
    form.confirm = ""
    saving.value = false
  }
)

function close() {
  emit("update:visible", false)
}

async function submit() {
  if (!form.oldPassword) {
    ElMessage.warning("请输入原密码")
    return
  }
  if (form.newPassword.length < 6) {
    ElMessage.warning("新密码至少需要 6 位")
    return
  }
  if (form.newPassword !== form.confirm) {
    ElMessage.warning("两次输入的新密码不一致")
    return
  }
  saving.value = true
  try {
    await api.changePassword(form.oldPassword, form.newPassword)
    ElMessage.success("密码修改成功，下次登录请使用新密码")
    emit("changed")
    close()
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    saving.value = false
  }
}
</script>

<template>
  <el-dialog
    :model-value="visible"
    class="dc-dialog pwd-dialog"
    width="420px"
    :show-close="false"
    :close-on-click-modal="false"
    append-to-body
    @update:model-value="(v: boolean) => emit('update:visible', v)"
  >
    <div class="pwd-card">
      <header class="pwd-head">
        <h3>修改密码</h3>
        <p>需要先校验原密码，修改后立即生效</p>
      </header>

      <label class="field-label">原密码</label>
      <el-input
        v-model="form.oldPassword"
        size="large"
        type="password"
        show-password
        placeholder="请输入当前密码"
        autocomplete="off"
      />

      <label class="field-label">新密码</label>
      <el-input
        v-model="form.newPassword"
        size="large"
        type="password"
        show-password
        placeholder="至少 6 位"
        autocomplete="new-password"
      />

      <label class="field-label">确认新密码</label>
      <el-input
        v-model="form.confirm"
        size="large"
        type="password"
        show-password
        placeholder="再次输入新密码"
        autocomplete="new-password"
        @keyup.enter="submit"
      />

      <div class="pwd-actions">
        <button class="ghost-btn" :disabled="saving" @click="close">取消</button>
        <el-button type="primary" size="large" :loading="saving" class="ok-btn" @click="submit">
          确认修改
        </el-button>
      </div>
    </div>
  </el-dialog>
</template>

<style scoped>
.pwd-card {
  padding: 24px 24px 20px;
}

.pwd-head h3 {
  margin: 0;
  font-size: 17px;
  font-weight: 600;
  color: #eaf6f3;
}

.pwd-head p {
  margin: 6px 0 4px;
  font-size: 11.5px;
  color: var(--text-mute);
}

.field-label {
  display: block;
  margin: 14px 0 7px;
  font-size: 12px;
  color: var(--text-dim);
}

.pwd-actions {
  margin-top: 22px;
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
}

.ghost-btn {
  height: 40px;
  padding: 0 16px;
  border-radius: 8px;
  border: 1px solid #24343e;
  background: transparent;
  color: var(--text-dim);
  font-size: 13px;
  cursor: pointer;
}

.ghost-btn:hover {
  border-color: #334954;
  color: #dbe7ef;
}

.ok-btn {
  height: 40px;
  padding: 0 20px;
}
</style>
