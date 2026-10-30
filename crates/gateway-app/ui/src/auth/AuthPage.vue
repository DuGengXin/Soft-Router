<template>
  <main class="auth-stage">
    <section class="signal-panel" aria-label="Gateway-Kit 控制面">
      <div class="signal-head"><span class="signal-dot"></span> GATEWAY-KIT / CONTROL PLANE</div>
      <div class="route-map" aria-hidden="true">
        <span class="node node-wan">WAN</span>
        <span class="node node-core">GK</span>
        <span class="node node-lan">LAN</span>
        <i class="wire wire-a"></i><i class="wire wire-b"></i>
        <i class="wire wire-c"></i><i class="wire wire-d"></i>
      </div>
      <div class="signal-copy">
        <p class="eyebrow">NETWORK OPERATIONS</p>
        <h1>把网络状态<br />掌握在本机。</h1>
        <p>登录后管理接口、出口、分流和应用变更。</p>
      </div>
      <div class="signal-foot"><span>LOCAL ADMINISTRATION</span><span>SECURE SESSION · 8H</span></div>
    </section>

    <section class="login-panel">
      <div class="login-card">
        <div class="mobile-mark"><span class="mark">GK</span> Gateway-Kit</div>
        <template v-if="mustChange">
          <p class="eyebrow">FIRST SIGN-IN</p>
          <h2>设置新的管理员密码</h2>
          <p class="intro">临时密码仅用于首次登录。设置新密码后才能打开控制台。</p>
          <el-form label-position="top" @submit.prevent="changePassword">
            <el-form-item label="管理员用户名">
              <el-input v-model="usernameValue" autocomplete="username" maxlength="32" />
            </el-form-item>
            <el-form-item label="临时密码">
              <el-input v-model="currentPassword" type="password" show-password autocomplete="current-password" />
            </el-form-item>
            <el-form-item label="新密码">
              <el-input v-model="newPassword" type="password" show-password autocomplete="new-password" />
            </el-form-item>
            <el-form-item label="确认新密码">
              <el-input v-model="confirmPassword" type="password" show-password autocomplete="new-password" @keyup.enter="changePassword" />
            </el-form-item>
            <p class="password-note">至少 12 个字符。密码只保存为 Argon2id 哈希。</p>
            <el-alert v-if="error" :title="error" type="error" :closable="false" show-icon />
            <el-button class="submit" type="primary" :loading="busy" @click="changePassword">保存并进入控制台</el-button>
          </el-form>
        </template>
        <template v-else>
          <p class="eyebrow">ADMINISTRATOR SIGN-IN</p>
          <h2>登录控制面</h2>
          <p class="intro">使用网关管理员账户继续。</p>
          <el-form label-position="top" @submit.prevent="login">
            <el-form-item label="用户名">
              <el-input v-model="usernameValue" autocomplete="username" autofocus />
            </el-form-item>
            <el-form-item label="密码">
              <el-input v-model="currentPassword" type="password" show-password autocomplete="current-password" @keyup.enter="login" />
            </el-form-item>
            <el-alert v-if="error" :title="error" type="error" :closable="false" show-icon />
            <el-button class="submit" type="primary" :loading="busy" @click="login">登录</el-button>
          </el-form>
          <p class="recovery">忘记密码？在网关本机运行 <code>gateway-kit auth-reset-password</code></p>
        </template>
      </div>
      <div class="login-bottom">网页登录 · 管理接口需要有效会话</div>
    </section>
  </main>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { api } from "../api/client";

const props = defineProps<{ mustChange?: boolean; username?: string }>();
const emit = defineEmits<{ authenticated: [value: { username: string; password_change_required: boolean }] }>();
const usernameValue = ref(props.username || "");
const currentPassword = ref("");
const newPassword = ref("");
const confirmPassword = ref("");
const error = ref("");
const busy = ref(false);

watch(() => props.username, (value) => { usernameValue.value = value || ""; });

async function login() {
  error.value = "";
  busy.value = true;
  try {
    const result = await api("/api/v1/auth/login", {
      method: "POST",
      body: JSON.stringify({ username: usernameValue.value.trim(), password: currentPassword.value }),
    }) as { username: string; password_change_required: boolean };
    if (!result.password_change_required) currentPassword.value = "";
    emit("authenticated", result);
  } catch {
    error.value = "用户名或密码错误，或登录请求暂时受限。";
  } finally {
    busy.value = false;
  }
}

async function changePassword() {
  error.value = "";
  if (newPassword.value.length < 12) {
    error.value = "新密码至少需要 12 个字符。";
    return;
  }
  if (newPassword.value !== confirmPassword.value) {
    error.value = "两次输入的新密码不一致。";
    return;
  }
  busy.value = true;
  try {
    const result = await api("/api/v1/auth/credentials", {
      method: "PUT",
      body: JSON.stringify({
        username: usernameValue.value.trim(),
        current_password: currentPassword.value,
        new_password: newPassword.value,
      }),
    }) as { username: string; password_change_required: boolean };
    currentPassword.value = "";
    newPassword.value = "";
    confirmPassword.value = "";
    emit("authenticated", result);
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : "密码更新失败。";
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.auth-stage { min-height: 100vh; display: grid; grid-template-columns: minmax(340px, 0.92fr) minmax(420px, 1.08fr); background: #11191d; color: #e7eeec; }
.signal-panel { position: relative; overflow: hidden; display: flex; flex-direction: column; justify-content: space-between; padding: 34px clamp(28px, 5vw, 72px); border-right: 1px solid #29363a; background: radial-gradient(circle at 50% 36%, #1b302e 0, #142124 42%, #11191d 78%); }
.signal-panel::after { position: absolute; inset: 0; content: ""; pointer-events: none; opacity: .18; background-image: linear-gradient(#56706c 1px, transparent 1px), linear-gradient(90deg, #56706c 1px, transparent 1px); background-size: 48px 48px; mask-image: linear-gradient(transparent, #000 25%, #000 75%, transparent); }
.signal-head, .signal-foot, .eyebrow { position: relative; z-index: 1; font: 600 11px/1.4 ui-monospace, SFMono-Regular, Consolas, monospace; letter-spacing: .16em; }
.signal-head { color: #9cb0aa; }
.signal-dot { display: inline-block; width: 7px; height: 7px; margin-right: 9px; border-radius: 50%; background: #89d7b3; box-shadow: 0 0 12px #89d7b3; }
.route-map { position: absolute; inset: 18% 9% auto; height: 34%; z-index: 1; }
.route-map::before, .route-map::after { position: absolute; content: ""; background: #3a625a; }
.route-map::before { left: 18%; right: 18%; top: 50%; height: 1px; }
.route-map::after { top: 18%; bottom: 18%; left: 50%; width: 1px; }
.node { position: absolute; z-index: 2; display: grid; place-items: center; width: 58px; height: 58px; border: 1px solid #4d796c; border-radius: 50%; background: #152320; color: #b8d0c7; font: 600 10px ui-monospace, monospace; box-shadow: 0 0 0 8px #14211f99; }
.node-wan { left: 9%; top: calc(50% - 29px); }.node-core { left: calc(50% - 29px); top: calc(50% - 29px); background: #a4dfbd; border-color: #a4dfbd; color: #15211d; }.node-lan { right: 9%; top: calc(50% - 29px); }
.wire { position: absolute; z-index: 1; height: 5px; width: 5px; border-radius: 50%; background: #b9e6cb; box-shadow: 0 0 10px #a4dfbd; }.wire-a { left: 29%; top: 49%; }.wire-b { left: 39%; top: 49%; }.wire-c { right: 29%; top: 49%; }.wire-d { right: 39%; top: 49%; }
.signal-copy { position: relative; z-index: 1; margin-top: auto; padding-bottom: 36px; }.signal-copy .eyebrow { color: #92b5a6; }.signal-copy h1 { margin: 18px 0; font-size: clamp(32px, 4vw, 54px); line-height: 1.13; letter-spacing: -.04em; font-weight: 640; }.signal-copy p:last-child { color: #91a19e; font-size: 14px; }
.signal-foot { display: flex; justify-content: space-between; color: #60736e; font-size: 9px; }
.login-panel { display: grid; place-items: center; padding: 48px 30px 22px; }.login-card { width: min(100%, 420px); }.eyebrow { color: #70bf98; }.login-card h2 { margin: 12px 0 8px; color: #f1f5f3; font-size: 30px; letter-spacing: -.03em; }.intro { margin: 0 0 30px; color: #9baaa6; font-size: 14px; }.submit { width: 100%; height: 44px; margin-top: 20px; font-weight: 700; }.password-note, .recovery { color: #7e908a; font-size: 12px; line-height: 1.7; }.recovery { margin-top: 24px; }.recovery code { color: #b7d7c7; background: #1c2a28; padding: 2px 5px; border-radius: 3px; }.login-bottom { align-self: end; color: #586964; font: 10px ui-monospace, monospace; letter-spacing: .08em; }.mobile-mark { display: none; }
:deep(.el-form-item__label) { color: #bac7c2; }.login-card :deep(.el-input__wrapper) { background: #182326; box-shadow: 0 0 0 1px #344744 inset; }.login-card :deep(.el-input__inner) { color: #edf4f0; }.login-card :deep(.el-button--primary) { --el-button-bg-color: #8bd2ae; --el-button-border-color: #8bd2ae; --el-button-hover-bg-color: #a3dfbf; --el-button-hover-border-color: #a3dfbf; --el-button-text-color: #14221c; }
@media (max-width: 760px) { .auth-stage { grid-template-columns: 1fr; }.signal-panel { display: none; }.login-panel { min-height: 100vh; padding: 26px 24px; }.mobile-mark { display: flex; align-items: center; gap: 10px; margin-bottom: 56px; color: #dce8e2; font-weight: 700; }.mark { display: grid; place-items: center; width: 32px; height: 32px; border-radius: 7px; background: #a4dfbd; color: #14221c; font: 800 12px ui-monospace, monospace; }.login-bottom { margin-top: 52px; } }
</style>
