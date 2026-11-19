<script setup lang="ts">
import { onMounted, ref } from "vue"
import { open } from "@tauri-apps/plugin-dialog"
import { ElMessage } from "element-plus"
import * as api from "@/api"
import { i18n, setLocale, t, tErr } from "@/i18n"
import type { LanSettings } from "@/types"

const emit = defineEmits<{ close: [] }>()

const settings = ref<LanSettings | null>(null)
const busy = ref(false)

const locales: { key: "zh" | "en"; labelKey: string }[] = [
  { key: "zh", labelKey: "lang.zh" },
  { key: "en", labelKey: "lang.en" },
]

async function load() {
  try {
    settings.value = await api.lanSettings()
  } catch (err) {
    ElMessage.error(tErr(err))
  }
}

function chooseLocale(key: "zh" | "en") {
  setLocale(key)
}

async function chooseDir() {
  try {
    const picked = await open({
      directory: true,
      multiple: false,
      title: t("settings.transfer.pickTitle"),
      defaultPath: settings.value?.saveDir,
    })
    if (typeof picked !== "string" || !picked) return
    busy.value = true
    await api.lanCheckDir(picked)
    settings.value = await api.lanSetSaveDir(picked)
    ElMessage.success(t("settings.transfer.saved"))
  } catch (err) {
    ElMessage.error(t("settings.transfer.checkFailed", { error: tErr(err) }))
  } finally {
    busy.value = false
  }
}

async function resetDir() {
  busy.value = true
  try {
    settings.value = await api.lanResetSaveDir()
    ElMessage.success(t("settings.transfer.resetDone"))
  } catch (err) {
    ElMessage.error(tErr(err))
  } finally {
    busy.value = false
  }
}

onMounted(load)
</script>

<template>
  <section class="settings-page">
    <header class="settings-head">
      <button class="back-btn" :title="t('settings.back')" @click="emit('close')">
        <svg width="15" height="15" viewBox="0 0 24 24" fill="none">
          <path
            d="M15 5l-7 7 7 7"
            stroke="currentColor"
            stroke-width="1.7"
            stroke-linecap="round"
            stroke-linejoin="round"
          />
        </svg>
        {{ t("settings.back") }}
      </button>
      <div class="head-text">
        <h2>{{ t("settings.title") }}</h2>
        <p>{{ t("settings.subtitle") }}</p>
      </div>
    </header>

    <div class="settings-body">
      <section class="card">
        <div class="card-head">
          <h3>{{ t("settings.section.language") }}</h3>
          <p>{{ t("settings.section.languageHint") }}</p>
        </div>
        <div class="lang-row">
          <button
            v-for="item in locales"
            :key="item.key"
            class="lang-option"
            :class="{ active: i18n.locale === item.key }"
            @click="chooseLocale(item.key)"
          >
            <span class="lang-dot" :class="{ on: i18n.locale === item.key }"></span>
            {{ t(item.labelKey) }}
          </button>
        </div>
      </section>

      <section class="card">
        <div class="card-head">
          <h3>{{ t("settings.section.transfer") }}</h3>
          <p>{{ t("settings.transfer.saveHint") }}</p>
        </div>

        <div class="dir-box">
          <span class="dir-label">{{ t("settings.transfer.current") }}</span>
          <span class="dir-path mono" :title="settings?.saveDir || ''">{{ settings?.saveDir || "—" }}</span>
        </div>

        <div v-if="settings?.fallback" class="warn">{{ t("settings.transfer.fallback") }}</div>

        <div class="dir-actions">
          <button class="primary-btn" :disabled="busy" @click="chooseDir">
            {{ t("settings.transfer.choose") }}
          </button>
          <button class="ghost-btn" :disabled="busy || !settings?.custom" @click="resetDir">
            {{ t("settings.transfer.reset") }}
          </button>
          <span class="default-hint">
            {{ t("settings.transfer.default") }}
            <span class="mono">{{ settings?.defaultDir || "" }}</span>
          </span>
        </div>
      </section>
    </div>
  </section>
</template>

<style scoped>
.settings-page {
  position: absolute;
  inset: 0;
  z-index: 60;
  display: flex;
  flex-direction: column;
  background: radial-gradient(900px 420px at 24% -12%, rgba(45, 212, 191, 0.06), transparent 62%),
    var(--bg-0);
  animation: settingsIn 0.22s ease both;
}

@keyframes settingsIn {
  from {
    opacity: 0;
    transform: translateY(6px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}

.settings-head {
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

.settings-body {
  flex: 1;
  min-height: 0;
  overflow-y: auto;
  padding: 4px 26px 26px;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.card {
  border-radius: 12px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #0d171d, #0a1116);
  padding: 18px 20px 20px;
}

.card-head h3 {
  margin: 0;
  font-size: 14px;
  font-weight: 600;
  color: #dff2ee;
}

.card-head p {
  margin: 5px 0 0;
  font-size: 11.5px;
  color: var(--text-mute);
}

.lang-row {
  display: flex;
  gap: 10px;
  margin-top: 14px;
  flex-wrap: wrap;
}

.lang-option {
  display: inline-flex;
  align-items: center;
  gap: 8px;
  height: 34px;
  padding: 0 16px;
  border-radius: 9px;
  border: 1px solid #24343e;
  background: #0c151b;
  color: #a8c1cc;
  font-size: 12.5px;
  cursor: pointer;
  transition: all 0.15s ease;
}

.lang-option:hover {
  border-color: #2f4653;
  color: #dff2ee;
}

.lang-option.active {
  border-color: rgba(45, 212, 191, 0.5);
  background: rgba(45, 212, 191, 0.1);
  color: #85f5e7;
}

.lang-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  border: 1px solid #3b5560;
  background: transparent;
}

.lang-dot.on {
  border-color: #2dd4bf;
  background: #2dd4bf;
}

.dir-box {
  margin-top: 14px;
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
  padding: 10px 12px;
  border-radius: 9px;
  border: 1px solid #1d2c35;
  background: #0a1218;
}

.dir-label {
  flex: 0 0 auto;
  font-size: 11.5px;
  color: #5e7a87;
}

.dir-path {
  min-width: 0;
  flex: 1;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
  font-size: 12px;
  color: #bfe5df;
}

.warn {
  margin-top: 10px;
  padding: 8px 12px;
  border-radius: 8px;
  border: 1px solid rgba(251, 191, 36, 0.3);
  background: rgba(251, 191, 36, 0.08);
  color: #f8d190;
  font-size: 11.5px;
}

.dir-actions {
  display: flex;
  align-items: center;
  gap: 10px;
  margin-top: 14px;
  flex-wrap: wrap;
}

.primary-btn {
  height: 32px;
  padding: 0 14px;
  border-radius: 8px;
  border: 1px solid rgba(45, 212, 191, 0.4);
  background: rgba(45, 212, 191, 0.12);
  color: #7ff2e2;
  font-size: 12.5px;
  font-weight: 600;
  cursor: pointer;
}

.primary-btn:hover:not(:disabled) {
  background: rgba(45, 212, 191, 0.2);
  color: #d6fff9;
}

.primary-btn:disabled {
  opacity: 0.5;
  cursor: default;
}

.ghost-btn {
  height: 32px;
  padding: 0 13px;
  border-radius: 8px;
  border: 1px solid #24343e;
  background: transparent;
  color: var(--text-dim);
  font-size: 12.5px;
  cursor: pointer;
}

.ghost-btn:hover:not(:disabled) {
  border-color: rgba(45, 212, 191, 0.4);
  color: #d9fffa;
}

.ghost-btn:disabled {
  opacity: 0.45;
  cursor: default;
}

.default-hint {
  margin-left: auto;
  font-size: 11px;
  color: #4d6470;
  min-width: 0;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.default-hint .mono {
  font-family: var(--font-mono);
  color: #6d8896;
}
</style>
