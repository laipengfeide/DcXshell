<script setup lang="ts">
import { ref } from "vue"
import { ElMessage } from "element-plus"
import * as api from "@/api"
import type { UserInfo } from "@/types"

const props = defineProps<{ canBack: boolean }>()
const emit = defineEmits<{
  success: [UserInfo]
  guest: []
  back: []
}>()

const mode = ref<"login" | "register">("login")
const username = ref("")
const password = ref("")
const loading = ref(false)

function switchMode(next: "login" | "register") {
  mode.value = next
  password.value = ""
}

async function submit() {
  const name = username.value.trim()
  if (!name) {
    ElMessage.warning("请输入用户名")
    return
  }
  if (mode.value === "register" && name.length < 3) {
    ElMessage.warning("用户名至少 3 个字符")
    return
  }
  if (password.value.length < 6) {
    ElMessage.warning("密码至少需要 6 位")
    return
  }
  loading.value = true
  try {
    const user =
      mode.value === "login"
        ? await api.login(name, password.value)
        : await api.register(name, password.value)
    ElMessage.success(mode.value === "login" ? `欢迎回来，${user.username}` : "注册成功，已自动登录")
    emit("success", user)
  } catch (err) {
    ElMessage.error(String(err))
  } finally {
    loading.value = false
  }
}
</script>

<template>
  <div class="login">
    <section class="hero">
      <div class="glow glow-a"></div>
      <div class="glow glow-b"></div>
      <div class="grid-mask"></div>

      <div class="hero-content">
        <div class="hero-brand">
          <span class="hero-mark">&gt;_</span>
          <span class="hero-title">Dc <b>Xshell</b></span>
        </div>
        <p class="tagline">轻量、稳定、无需 USB Key 的 SSH 终端</p>
        <p class="hero-desc">
          为运维、测试与研发打造的桌面连接工具。连接前先自检，掉线自动重连，<br />
          像使用本地终端一样顺手。
        </p>

        <ul class="feature-list">
          <li>
            <span class="fi">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                <path d="M12 3l7 3v6c0 4.5-3 7.5-7 9-4-1.5-7-4.5-7-9V6l7-3z" stroke="#2dd4bf" stroke-width="1.6" />
                <path d="M9 12l2 2 4-4" stroke="#2dd4bf" stroke-width="1.6" />
              </svg>
            </span>
            <div>
              <strong>免 USB Key</strong>
              <p>账号、密钥与连接配置全部保存在本机 SQLite，随开随用。</p>
            </div>
          </li>
          <li>
            <span class="fi">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                <path d="M3 12h4l2-6 4 12 2-6h6" stroke="#60a5fa" stroke-width="1.6" />
              </svg>
            </span>
            <div>
              <strong>心跳保活</strong>
              <p>15 秒 SSH 保活、5 秒心跳探测，断线自动重连 3 次。</p>
            </div>
          </li>
          <li>
            <span class="fi">
              <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
                <rect x="3" y="5" width="8" height="6" stroke="#fbbf24" stroke-width="1.6" />
                <rect x="13" y="13" width="8" height="6" stroke="#fbbf24" stroke-width="1.6" />
                <path d="M11 8h4v8" stroke="#fbbf24" stroke-width="1.6" />
              </svg>
            </span>
            <div>
              <strong>多窗口并行</strong>
              <p>访客最多 3 个并行窗口，登录后无限打开。</p>
            </div>
          </li>
        </ul>
      </div>
    </section>

    <section class="panel">
      <div class="form-card">
        <header class="form-head">
          <h2>{{ mode === "login" ? "登录 Dc Xshell" : "创建本地账户" }}</h2>
          <p>
            {{ mode === "login" ? "登录后不限并行窗口数量" : "账户仅保存在本机，不会上传" }}
          </p>
        </header>

        <div class="seg">
          <button :class="{ active: mode === 'login' }" @click="switchMode('login')">登录</button>
          <button :class="{ active: mode === 'register' }" @click="switchMode('register')">注册</button>
        </div>

        <label class="field-label">用户名</label>
        <el-input
          v-model="username"
          size="large"
          placeholder="请输入用户名"
          autocomplete="off"
          @keyup.enter="submit"
        />

        <label class="field-label">密码</label>
        <el-input
          v-model="password"
          size="large"
          type="password"
          show-password
          placeholder="至少 6 位"
          @keyup.enter="submit"
        />

        <el-button class="submit" type="primary" size="large" :loading="loading" @click="submit">
          {{ mode === "login" ? "登录" : "注册并登录" }}
        </el-button>

        <div class="local-hint">
          <svg width="13" height="13" viewBox="0 0 24 24" fill="none">
            <rect x="4" y="10" width="16" height="10" rx="2" stroke="#5e7280" stroke-width="1.6" />
            <path d="M8 10V7a4 4 0 018 0v3" stroke="#5e7280" stroke-width="1.6" />
          </svg>
          本地账户 · 密码使用 Argon2 加密存储
        </div>

        <div class="divider"><span>或</span></div>

        <button class="guest-btn" @click="emit('guest')">
          以访客身份进入
          <span class="guest-note">最多 3 个并行窗口</span>
        </button>

        <button v-if="props.canBack" class="back-btn" @click="emit('back')">返回工作台</button>
      </div>

      <footer class="panel-foot">Dc Xshell · Desktop SSH Client</footer>
    </section>
  </div>
</template>

<style scoped>
.login {
  height: 100%;
  display: grid;
  grid-template-columns: minmax(0, 1.08fr) minmax(0, 0.92fr);
  background: radial-gradient(1200px 700px at 12% 18%, rgba(20, 184, 166, 0.1), transparent 60%),
    radial-gradient(900px 600px at 85% 80%, rgba(37, 99, 235, 0.1), transparent 60%),
    var(--bg-0);
  overflow: hidden;
}

.hero {
  position: relative;
  padding: 54px 56px;
  display: flex;
  align-items: center;
  overflow: hidden;
}

.glow {
  position: absolute;
  border-radius: 50%;
  filter: blur(70px);
  pointer-events: none;
}

.glow-a {
  width: 380px;
  height: 380px;
  background: rgba(16, 185, 129, 0.16);
  top: 6%;
  left: 4%;
}

.glow-b {
  width: 320px;
  height: 320px;
  background: rgba(56, 189, 248, 0.12);
  bottom: 4%;
  right: 8%;
}

.grid-mask {
  position: absolute;
  inset: 0;
  background-image: linear-gradient(rgba(45, 212, 191, 0.05) 1px, transparent 1px),
    linear-gradient(90deg, rgba(45, 212, 191, 0.05) 1px, transparent 1px);
  background-size: 48px 48px;
  mask-image: radial-gradient(circle at 40% 45%, #000 30%, transparent 75%);
  pointer-events: none;
}

.hero-content {
  position: relative;
  z-index: 1;
  max-width: 560px;
  animation: rise 0.55s ease both;
}

.hero-brand {
  display: flex;
  align-items: center;
  gap: 14px;
}

.hero-mark {
  width: 52px;
  height: 52px;
  display: grid;
  place-items: center;
  border-radius: 14px;
  background: linear-gradient(135deg, #10b981, #0ea5e9);
  color: #04120f;
  font-family: var(--font-mono);
  font-weight: 700;
  font-size: 19px;
  box-shadow: 0 0 40px rgba(45, 212, 191, 0.4);
}

.hero-title {
  font-size: 30px;
  font-weight: 700;
  letter-spacing: 0.4px;
  color: #effbf8;
}

.hero-title b {
  color: var(--accent);
  font-weight: 700;
}

.tagline {
  margin: 22px 0 10px;
  font-size: 17px;
  font-weight: 600;
  color: #cfe4e1;
  letter-spacing: 0.3px;
}

.hero-desc {
  margin: 0;
  font-size: 13px;
  line-height: 1.9;
  color: var(--text-dim);
}

.feature-list {
  list-style: none;
  margin: 34px 0 0;
  padding: 0;
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.feature-list li {
  display: flex;
  gap: 13px;
  align-items: flex-start;
}

.fi {
  width: 32px;
  height: 32px;
  flex: 0 0 32px;
  display: grid;
  place-items: center;
  border-radius: 9px;
  background: rgba(255, 255, 255, 0.035);
  border: 1px solid rgba(255, 255, 255, 0.06);
}

.feature-list strong {
  display: block;
  font-size: 13px;
  color: #d8e8e4;
  font-weight: 600;
}

.feature-list p {
  margin: 4px 0 0;
  font-size: 12px;
  color: var(--text-mute);
  line-height: 1.6;
}

.panel {
  position: relative;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  padding: 40px;
  border-left: 1px solid rgba(255, 255, 255, 0.045);
  background: linear-gradient(180deg, rgba(13, 21, 27, 0.6), rgba(7, 11, 14, 0.8));
}

.form-card {
  width: 100%;
  max-width: 372px;
  padding: 30px 28px 24px;
  border-radius: 16px;
  border: 1px solid var(--border);
  background: linear-gradient(180deg, #101a21, #0c141a);
  box-shadow: 0 26px 70px rgba(0, 0, 0, 0.45), inset 0 1px 0 rgba(255, 255, 255, 0.03);
  animation: rise 0.55s 0.08s ease both;
}

.form-head h2 {
  margin: 0;
  font-size: 19px;
  font-weight: 600;
  color: #eaf6f3;
  letter-spacing: 0.3px;
}

.form-head p {
  margin: 7px 0 0;
  font-size: 12px;
  color: var(--text-mute);
}

.seg {
  margin: 22px 0 20px;
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px;
  padding: 4px;
  border-radius: 10px;
  background: #0a1217;
  border: 1px solid #1a2831;
}

.seg button {
  height: 32px;
  border: 0;
  border-radius: 7px;
  background: transparent;
  color: var(--text-dim);
  font-size: 13px;
  cursor: pointer;
  transition: all 0.18s ease;
}

.seg button.active {
  background: linear-gradient(180deg, #142b2a, #102322);
  color: var(--accent);
  box-shadow: inset 0 0 0 1px rgba(45, 212, 191, 0.24);
}

.field-label {
  display: block;
  margin: 14px 0 7px;
  font-size: 12px;
  color: var(--text-dim);
  letter-spacing: 0.2px;
}

.submit {
  width: 100%;
  margin-top: 22px;
  height: 42px;
  font-size: 14px;
  letter-spacing: 1px;
}

.local-hint {
  margin-top: 14px;
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 11px;
  color: var(--text-mute);
}

.divider {
  position: relative;
  text-align: center;
  margin: 20px 0 16px;
}

.divider::before {
  content: "";
  position: absolute;
  left: 0;
  right: 0;
  top: 50%;
  height: 1px;
  background: #1b2830;
}

.divider span {
  position: relative;
  padding: 0 10px;
  background: #0e171d;
  font-size: 11px;
  color: var(--text-mute);
}

.guest-btn {
  width: 100%;
  height: 44px;
  border-radius: 10px;
  border: 1px solid #22323c;
  background: transparent;
  color: #b9cdd7;
  font-size: 13px;
  cursor: pointer;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  transition: all 0.18s ease;
}

.guest-btn:hover {
  border-color: rgba(45, 212, 191, 0.4);
  color: #e7faf5;
  background: rgba(45, 212, 191, 0.05);
}

.guest-note {
  font-size: 11px;
  color: var(--text-mute);
}

.back-btn {
  width: 100%;
  margin-top: 10px;
  height: 34px;
  border: 0;
  background: transparent;
  color: var(--text-mute);
  font-size: 12px;
  cursor: pointer;
}

.back-btn:hover {
  color: var(--text-dim);
}

.panel-foot {
  position: absolute;
  bottom: 18px;
  font-size: 11px;
  color: #3c4d58;
  letter-spacing: 0.4px;
}

@keyframes rise {
  from {
    opacity: 0;
    transform: translateY(12px);
  }
  to {
    opacity: 1;
    transform: none;
  }
}
</style>
