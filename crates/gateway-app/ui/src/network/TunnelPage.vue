<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>隧道 · 到 VPS（WireGuard / VLESS）</template>
    <el-form v-if="form" :model="form" label-width="140px">
      <el-form-item label="出口方式">
        <el-radio-group v-model="transport" class="transport-group">
          <el-radio-button value="none">不启用</el-radio-button>
          <el-radio-button value="wireguard">WireGuard</el-radio-button>
          <el-radio-button value="vless">VLESS 链接</el-radio-button>
        </el-radio-group>
        <div class="hint">
          网关模式必须选择一种出口；WireGuard 与 VLESS 只能二选一，保存时会清除另一方的凭证。
        </div>
      </el-form-item>

      <template v-if="transport === 'vless'">
        <el-form-item label="VLESS/Xray 链接">
          <el-input v-model="proxyUri" type="textarea" :rows="3" placeholder="vless://..." />
          <div class="hint">
            链接直接写入 secrets.toml（600），是唯一的隧道凭证，无需再填 WireGuard 字段。
            计划页不会回显明文；当前支持 TCP + TLS/REALITY 传输。
          </div>
        </el-form-item>
      </template>

      <template v-if="transport === 'wireguard'">
        <el-form-item label="粘贴导入">
          <el-input v-model="blob" type="textarea" :rows="5" placeholder="wg-quick 配置，或其 Base64" />
          <el-button class="mt" @click="parseBlob">解析并填入</el-button>
          <div class="hint">私钥只进入 secrets.toml，不会写入计划 JSON。</div>
        </el-form-item>
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
      </template>

      <el-form-item label="凭证状态">{{ secretStatus }}</el-form-item>
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

type Transport = "none" | "wireguard" | "vless";

type SecretsStatus = {
  wireguard_private_key_present?: boolean;
  wireguard_peer_public_key_present?: boolean;
  proxy_uri_present?: boolean;
};

const loading = ref(true);
const form = ref<Cfg | null>(null);
const transport = ref<Transport>("none");
const blob = ref("");
const secretStatus = ref("未读取");
const wgPriv = ref("");
const wgPub = ref("");
const wgPsk = ref("");
const proxyUri = ref("");
const lastStatus = ref<SecretsStatus>({});

function describe(sec: SecretsStatus) {
  const parts: string[] = [];
  if (sec.proxy_uri_present) parts.push("VLESS 链接已存");
  if (sec.wireguard_private_key_present && sec.wireguard_peer_public_key_present) {
    parts.push("WireGuard 密钥已存");
  } else if (sec.wireguard_private_key_present || sec.wireguard_peer_public_key_present) {
    parts.push("WireGuard 密钥不完整");
  }
  return parts.length ? parts.join(" · ") : "尚未保存任何隧道凭证";
}

async function refreshSecrets() {
  const sec = (await api("/api/v1/secrets")) as SecretsStatus;
  lastStatus.value = sec;
  secretStatus.value = describe(sec);
  return sec;
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
  ElMessage.success("已解析，请核对后保存");
}

onMounted(async () => {
  try {
    form.value = await loadConfig();
    const sec = await refreshSecrets();
    if (form.value.wireguard.enabled) transport.value = "wireguard";
    else if (sec.proxy_uri_present) transport.value = "vless";
    else transport.value = "none";
  } finally {
    loading.value = false;
  }
});

async function save() {
  if (!form.value) return;
  if (transport.value === "vless" && !proxyUri.value.trim()) {
    const sec = await refreshSecrets();
    if (!sec.proxy_uri_present) {
      ElMessage.error("请粘贴 VLESS 链接");
      return;
    }
  }
  if (transport.value === "wireguard") {
    const sec = lastStatus.value;
    const hasKeys =
      (sec.wireguard_private_key_present || Boolean(wgPriv.value.trim())) &&
      (sec.wireguard_peer_public_key_present || Boolean(wgPub.value.trim()));
    if (!hasKeys) {
      ElMessage.error("WireGuard 需要本机私钥和对端公钥");
      return;
    }
  }
  form.value.wireguard.enabled = transport.value === "wireguard";

  const patch: Record<string, string> = {};
  if (wgPriv.value.trim()) patch.wireguard_private_key = wgPriv.value.trim();
  if (wgPub.value.trim()) patch.wireguard_peer_public_key = wgPub.value.trim();
  if (wgPsk.value.trim()) patch.wireguard_preshared_key = wgPsk.value.trim();
  if (transport.value === "vless" && proxyUri.value.trim()) {
    patch.proxy_uri = proxyUri.value.trim();
  }
  if (transport.value !== "vless") {
    // Empty string clears the stored link: the two transports are mutually exclusive.
    patch.proxy_uri = "";
  }
  await api("/api/v1/secrets", { method: "PUT", body: JSON.stringify(patch) });
  wgPriv.value = "";
  wgPub.value = "";
  wgPsk.value = "";
  proxyUri.value = "";
  await saveConfig(form.value);
  await refreshSecrets();
}
</script>

<style scoped>
.transport-group {
  display: block;
}
.hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  margin-top: 6px;
  flex-basis: 100%;
  width: 100%;
}
.mt {
  margin-top: 8px;
}
</style>
