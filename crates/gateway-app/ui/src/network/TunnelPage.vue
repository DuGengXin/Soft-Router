<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>隧道 · 到 VPS（WireGuard / VLESS）</template>
    <el-form v-if="form" :model="form" label-width="140px">
      <el-form-item label="启用">
        <el-switch v-model="form.wireguard.enabled" />
      </el-form-item>
      <el-form-item label="VLESS/Xray 链接">
        <el-input v-model="proxyUri" type="textarea" :rows="3" placeholder="vless://...（与 WireGuard 二选一）" />
        <div class="hint">链接只写入 secrets.toml（600）；与 WireGuard 二选一，计划页不会回显明文。</div>
      </el-form-item>
      <el-form-item label="粘贴导入">
        <el-input v-model="blob" type="textarea" :rows="5" placeholder="wg-quick 配置，或其 Base64" />
        <el-button class="mt" @click="parseBlob">解析并填入</el-button>
        <div class="hint">私钥只进入 secrets.toml，不会写入计划 JSON。</div>
      </el-form-item>
      <el-form-item label="密钥状态">{{ secretStatus }}</el-form-item>
      <el-form-item label="本机 WG 地址">
        <el-input v-model="form.wireguard.address" placeholder="10.66.0.2/32" />
      </el-form-item>
      <el-form-item label="WG 网卡名">
        <el-input v-model="form.wireguard.interface" placeholder="wg0" />
      </el-form-item>
      <el-form-item label="对端 Endpoint">
        <el-input v-model="form.wireguard.peer_endpoint" placeholder="vps:51820" />
      </el-form-item>
      <el-form-item label="ListenPort">
        <el-input v-model.number="form.wireguard.listen_port" />
      </el-form-item>
      <el-form-item label="AllowedIPs">
        <el-input v-model="form.wireguard.peer_allowed_ips" />
      </el-form-item>
      <el-form-item label="本机 PrivateKey">
        <el-input v-model="wgPriv" type="password" show-password autocomplete="new-password" />
      </el-form-item>
      <el-form-item label="对端 PublicKey">
        <el-input v-model="wgPub" type="password" show-password autocomplete="new-password" />
      </el-form-item>
      <el-form-item label="PSK（可选）">
        <el-input v-model="wgPsk" type="password" show-password autocomplete="new-password" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </el-card>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { api, ElMessage } from "../api/client";
import { type Cfg, loadConfig, saveConfig } from "../api/config";

const loading = ref(true);
const form = ref<Cfg | null>(null);
const blob = ref("");
const secretStatus = ref("未读取");
const wgPriv = ref("");
const wgPub = ref("");
const wgPsk = ref("");
const proxyUri = ref("");

async function refreshSecrets() {
  const sec = (await api("/api/v1/secrets")) as {
    wireguard_private_key_present?: boolean;
    wireguard_peer_public_key_present?: boolean;
    proxy_uri_present?: boolean;
  };
  secretStatus.value = [
    sec.wireguard_private_key_present ? "本机私钥已存" : "缺私钥",
    sec.wireguard_peer_public_key_present ? "对端公钥已存" : "缺对端公钥",
    sec.proxy_uri_present ? "VLESS 链接已存" : "未配置 VLESS",
  ].join(" · ");
}

async function parseBlob() {
  const parsed = (await api("/api/v1/wireguard/parse", {
    method: "POST",
    body: JSON.stringify({ blob: blob.value }),
  })) as {
    address?: string;
    listen_port?: number;
    peer_endpoint?: string;
    peer_allowed_ips?: string;
    private_key?: string;
    peer_public_key?: string;
    preshared_key?: string;
  };
  if (!form.value) return;
  if (parsed.address) form.value.wireguard.address = parsed.address;
  if (parsed.listen_port) form.value.wireguard.listen_port = parsed.listen_port;
  if (parsed.peer_endpoint) form.value.wireguard.peer_endpoint = parsed.peer_endpoint;
  if (parsed.peer_allowed_ips) form.value.wireguard.peer_allowed_ips = parsed.peer_allowed_ips;
  if (parsed.private_key) wgPriv.value = parsed.private_key;
  if (parsed.peer_public_key) wgPub.value = parsed.peer_public_key;
  if (parsed.preshared_key) wgPsk.value = parsed.preshared_key;
  form.value.wireguard.enabled = true;
  ElMessage.success("已解析，请核对后保存");
}

onMounted(async () => {
  try {
    form.value = await loadConfig();
    await refreshSecrets();
  } finally {
    loading.value = false;
  }
});

async function save() {
  if (!form.value) return;
  const patch: Record<string, string> = {};
  if (wgPriv.value.trim()) patch.wireguard_private_key = wgPriv.value.trim();
  if (wgPub.value.trim()) patch.wireguard_peer_public_key = wgPub.value.trim();
  if (wgPsk.value.trim()) patch.wireguard_preshared_key = wgPsk.value.trim();
  if (proxyUri.value.trim()) patch.proxy_uri = proxyUri.value.trim();
  if (Object.keys(patch).length) {
    await api("/api/v1/secrets", { method: "PUT", body: JSON.stringify(patch) });
    wgPriv.value = "";
    wgPub.value = "";
    wgPsk.value = "";
    proxyUri.value = "";
  }
  await saveConfig(form.value);
  await refreshSecrets();
}
</script>

<style scoped>
.hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  margin-top: 6px;
}
.mt {
  margin-top: 8px;
}
</style>
