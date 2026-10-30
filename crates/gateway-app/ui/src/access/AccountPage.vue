<template>
  <div class="account-page">
    <div class="page-intro">
      <div>
        <p class="eyebrow">ADMINISTRATOR</p>
        <h1>账号与安全</h1>
        <p class="muted">登录凭据保存在网关本机。修改后，其他浏览器会话将立即失效。</p>
      </div>
      <el-tag type="success" effect="plain">当前账户：{{ username }}</el-tag>
    </div>

    <el-row :gutter="20">
      <el-col :xs="24" :lg="14">
        <el-card shadow="never" class="credential-card">
          <template #header>
            <div class="card-heading"><span class="key-mark">ID</span><div><strong>更新管理员凭据</strong><p>使用当前密码确认更改</p></div></div>
          </template>
          <el-form label-position="top" @submit.prevent="save">
            <el-form-item label="当前密码">
              <el-input v-model="currentPassword" type="password" show-password autocomplete="current-password" />
            </el-form-item>
            <el-divider content-position="left">新凭据</el-divider>
            <el-form-item label="用户名">
              <el-input v-model="newUsername" autocomplete="username" maxlength="32" />
              <div class="field-note">3–32 位字母、数字、点、横线或下划线</div>
            </el-form-item>
            <el-form-item label="新密码">
              <el-input v-model="newPassword" type="password" show-password autocomplete="new-password" />
              <div class="field-note">至少 12 个字符；服务端只保存 Argon2id 哈希</div>
            </el-form-item>
            <el-form-item label="确认新密码">
              <el-input v-model="confirmPassword" type="password" show-password autocomplete="new-password" @keyup.enter="save" />
            </el-form-item>
            <el-alert v-if="error" :title="error" type="error" :closable="false" show-icon />
            <el-button type="primary" :loading="busy" @click="save">保存凭据</el-button>
          </el-form>
        </el-card>
      </el-col>
      <el-col :xs="24" :lg="10">
        <el-card shadow="never" class="session-card">
          <template #header><strong>会话策略</strong></template>
          <div class="policy-row"><span>会话有效期</span><b>8 小时</b></div>
          <div class="policy-row"><span>Cookie</span><b>HttpOnly · SameSite Strict</b></div>
          <div class="policy-row"><span>登录失败</span><b>每 IP 每分钟最多 8 次</b></div>
          <p class="security-note">浏览器会话保存在服务进程内，服务重启后需要重新登录。公网访问请使用 HTTPS 反向代理或 VPN。</p>
        </el-card>
      </el-col>
    </el-row>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from "vue";
import { api, ElMessage } from "../api/client";

const props = defineProps<{ username: string }>();
const emit = defineEmits<{ updated: [username: string] }>();
const newUsername = ref(props.username);
const currentPassword = ref("");
const newPassword = ref("");
const confirmPassword = ref("");
const busy = ref(false);
const error = ref("");
watch(() => props.username, (value) => { newUsername.value = value; });

async function save() {
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
        username: newUsername.value.trim(),
        current_password: currentPassword.value,
        new_password: newPassword.value,
      }),
    }) as { username: string };
    currentPassword.value = "";
    newPassword.value = "";
    confirmPassword.value = "";
    emit("updated", result.username);
    ElMessage.success("管理员凭据已更新。其他会话已退出。");
  } catch (reason) {
    error.value = reason instanceof Error ? reason.message : "凭据更新失败。";
  } finally {
    busy.value = false;
  }
}
</script>

<style scoped>
.account-page { max-width: 1100px; margin: 0 auto; }.page-intro { display: flex; justify-content: space-between; align-items: center; gap: 20px; margin-bottom: 22px; }.eyebrow { margin: 0 0 6px; color: #70bf98; font: 600 10px ui-monospace, monospace; letter-spacing: .15em; }.page-intro h1 { margin: 0 0 6px; font-size: 26px; }.muted, .card-heading p, .field-note { color: var(--el-text-color-secondary); font-size: 13px; }.muted { margin: 0; }.credential-card, .session-card { border-color: var(--el-border-color-lighter); }.card-heading { display: flex; align-items: center; gap: 12px; }.card-heading p { margin: 4px 0 0; font-weight: 400; }.key-mark { display: grid; place-items: center; width: 34px; height: 34px; border: 1px solid #4c8069; border-radius: 7px; color: #8bd2ae; font: 700 11px ui-monospace, monospace; }.field-note { margin-top: 5px; font-size: 12px; }.policy-row { display: flex; justify-content: space-between; gap: 12px; padding: 13px 0; border-bottom: 1px solid var(--el-border-color-lighter); color: var(--el-text-color-secondary); font-size: 13px; }.policy-row b { color: var(--el-text-color-primary); font-weight: 600; text-align: right; }.security-note { margin: 18px 0 0; color: var(--el-text-color-secondary); font-size: 12px; line-height: 1.7; }
@media (max-width: 600px) { .page-intro { align-items: flex-start; flex-direction: column; } }
</style>
