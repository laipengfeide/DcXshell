<script setup lang="ts">
import { reactive, ref, watch } from "vue"
import { ElMessage } from "element-plus"
import * as api from "@/api"
import { t, tErr } from "@/i18n"

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
    ElMessage.warning(t("pwd.err.oldRequired"))
    return
  }
  if (form.newPassword.length < 6) {
    ElMessage.warning(t("pwd.err.newTooShort"))
    return
  }
  if (form.newPassword !== form.confirm) {
    ElMessage.warning(t("pwd.err.mismatch"))
    return
  }
  saving.value = true
  try {
    await api.changePassword(form.oldPassword, form.newPassword)
    ElMessage.success(t("pwd.ok"))
    emit("changed")
    close()
  } catch (err) {
    ElMessage.error(tErr(err))
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
        <h3>{{ t("pwd.title") }}</h3>
        <p>{{ t("pwd.subtitle") }}</p>
      </header>

      <label class="field-label">{{ t("pwd.label.old") }}</label>
      <el-input
        v-model="form.oldPassword"
        size="large"
        type="password"
        show-password
        :placeholder="t('pwd.ph.old')"
        autocomplete="off"
      />

      <label class="field-label">{{ t("pwd.label.new") }}</label>
      <el-input
        v-model="form.newPassword"
        size="large"
        type="password"
        show-password
        :placeholder="t('pwd.ph.new')"
        autocomplete="new-password"
      />

      <label class="field-label">{{ t("pwd.label.confirm") }}</label>
      <el-input
        v-model="form.confirm"
        size="large"
        type="password"
        show-password
        :placeholder="t('pwd.ph.confirm')"
        autocomplete="new-password"
        @keyup.enter="submit"
      />

      <div class="pwd-actions">
        <button class="ghost-btn" :disabled="saving" @click="close">{{ t("pwd.cancel") }}</button>
        <el-button type="primary" size="large" :loading="saving" class="ok-btn" @click="submit">
          {{ t("pwd.submit") }}
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
