<template>
  <el-card shadow="never">
    <template #header>LAN 访问令牌</template>
    <p class="hint">
      非 127.0.0.1 监听时，向导或 secrets.toml 必须已有 ui_lan_token。下面只把同一令牌存进本机浏览器，供请求头使用。
    </p>
    <el-form label-width="160px">
      <el-form-item label="x-gateway-token">
        <el-input v-model="value" type="password" show-password autocomplete="off" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="save">保存到浏览器</el-button>
      </el-form-item>
    </el-form>
  </el-card>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { ElMessage } from "../api/client";

const value = ref(localStorage.getItem("gk_token") || "");

function save() {
  const v = value.value.trim();
  if (v) localStorage.setItem("gk_token", v);
  else localStorage.removeItem("gk_token");
  ElMessage.success("令牌已写入本机浏览器。刷新后生效。");
  location.reload();
}
</script>

<style scoped>
.hint {
  color: var(--el-text-color-secondary);
  margin-bottom: 16px;
}
</style>
