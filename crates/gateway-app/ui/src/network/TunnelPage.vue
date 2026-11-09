<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>隧道 · 境外出口（WireGuard / VLESS / SOCKS 中继）</template>
    <el-form v-if="form" :model="form" label-width="140px">
      <el-form-item label="出口方式">
        <el-radio-group v-model="transport" class="transport-group">
          <el-radio-button value="none">不启用</el-radio-button>
          <el-radio-button value="wireguard">WireGuard</el-radio-button>
          <el-radio-button value="vless">VLESS 链接</el-radio-button>
          <el-radio-button value="socks">SOCKS 中继</el-radio-button>
        </el-radio-group>
        <div class="hint">
          网关模式必须选择一种出口。切换出口不会清空其它已存凭证，可一键切回。
          SOCKS 中继适用于经 EasyTier 等到家中/中继的 Mixed/SOCKS 端口。
        </div>
      </el-form-item>

      <template v-if="transport === 'vless'">
        <el-form-item label="VLESS/Xray 链接">
          <el-input v-model="proxyUri" type="textarea" :rows="3" placeholder="vless://..." />
          <div class="hint">
            留空并保存时保留已存链接。写入 secrets.toml（600），计划页不回显明文。
            当前支持 TCP + TLS/REALITY。
          </div>
        </el-form-item>
        <el-form-item>
          <el-button type="danger" plain :disabled="!lastStatus.proxy_uri_present" @click="clearVless">
            清除 VLESS 凭证
          </el-button>
        </el-form-item>
      </template>

      <template v-if="transport === 'socks'">
        <el-form-item label="SOCKS 主机">
          <el-input v-model="socksHost" placeholder="10.10.10.10" />
        </el-form-item>
        <el-form-item label="端口">
          <el-input v-model.number="socksPort" placeholder="10808" />
        </el-form-item>
        <el-form-item label="用户名（可选）">
          <el-input v-model="socksUser" autocomplete="off" />
        </el-form-item>
        <el-form-item label="密码（可选）">
          <el-input v-model="socksPass" type="password" show-password autocomplete="new-password" />
        </el-form-item>
        <div class="hint">
          保存时写入 socks5://…；若主机留空且已有凭证，则只切换出口不改地址。
        </div>
        <el-form-item>
          <el-button type="danger" plain :disabled="!lastStatus.socks_proxy_uri_present" @click="clearSocks">
            清除 SOCKS 凭证
          </el-button>
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

type Transport = "none" | "wireguard" | "vless" | "socks";

type SecretsStatus = {
  wireguard_private_key_present?: boolean;
  wireguard_peer_public_key_present?: boolean;
  proxy_uri_present?: boolean;
  socks_proxy_uri_present?: boolean;
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
const socksHost = ref("");
const socksPort = ref(10808);
const socksUser = ref("");
const socksPass = ref("");
const lastStatus = ref<SecretsStatus>({});

function describe(sec: SecretsStatus) {
  const parts: string[] = [];
  if (sec.proxy_uri_present) parts.push("VLESS 已存");
  if (sec.socks_proxy_uri_present) parts.push("SOCKS 已存");
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

function buildSocksUri(): string | null {
  const host = socksHost.value.trim();
  if (!host) return null;
  const port = Number(socksPort.value) || 10808;
  const user = socksUser.value.trim();
  const pass = socksPass.value;
  if (user) {
    const auth = pass ? `${encodeURIComponent(user)}:${encodeURIComponent(pass)}@` : `${encodeURIComponent(user)}@`;
    return `socks5://${auth}${host}:${port}`;
  }
  return `socks5://${host}:${port}`;
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

function resolveTransport(cfg: Cfg, sec: SecretsStatus): Transport {
  if (cfg.overseas_transport === "wireguard" || cfg.overseas_transport === "vless" || cfg.overseas_transport === "socks" || cfg.overseas_transport === "none") {
    return cfg.overseas_transport;
  }
  if (cfg.wireguard.enabled) return "wireguard";
  if (sec.proxy_uri_present) return "vless";
  if (sec.socks_proxy_uri_present) return "socks";
  return "none";
}

onMounted(async () => {
  try {
    form.value = await loadConfig();
    const sec = await refreshSecrets();
    transport.value = resolveTransport(form.value, sec);
  } finally {
    loading.value = false;
  }
});

async function clearVless() {
  await api("/api/v1/secrets", { method: "PUT", body: JSON.stringify({ proxy_uri: "" }) });
  proxyUri.value = "";
  await refreshSecrets();
  ElMessage.success("已清除 VLESS 凭证");
}

async function clearSocks() {
  await api("/api/v1/secrets", { method: "PUT", body: JSON.stringify({ socks_proxy_uri: "" }) });
  socksHost.value = "";
  socksUser.value = "";
  socksPass.value = "";
  await refreshSecrets();
  ElMessage.success("已清除 SOCKS 凭证");
}

async function save() {
  if (!form.value) return;
  if (transport.value === "vless" && !proxyUri.value.trim() && !lastStatus.value.proxy_uri_present) {
    ElMessage.error("请粘贴 VLESS 链接");
    return;
  }
  if (transport.value === "socks") {
    const built = buildSocksUri();
    if (!built && !lastStatus.value.socks_proxy_uri_present) {
      ElMessage.error("请填写 SOCKS 主机与端口");
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

  form.value.overseas_transport = transport.value;
  form.value.wireguard.enabled = transport.value === "wireguard";

  const patch: Record<string, string> = {};
  if (wgPriv.value.trim()) patch.wireguard_private_key = wgPriv.value.trim();
  if (wgPub.value.trim()) patch.wireguard_peer_public_key = wgPub.value.trim();
  if (wgPsk.value.trim()) patch.wireguard_preshared_key = wgPsk.value.trim();
  if (transport.value === "vless" && proxyUri.value.trim()) {
    patch.proxy_uri = proxyUri.value.trim();
  }
  if (transport.value === "socks") {
    const built = buildSocksUri();
    if (built) patch.socks_proxy_uri = built;
  }
  // Do not clear other credential slots when switching transport.
  if (Object.keys(patch).length > 0) {
    await api("/api/v1/secrets", { method: "PUT", body: JSON.stringify(patch) });
  }
  wgPriv.value = "";
  wgPub.value = "";
  wgPsk.value = "";
  proxyUri.value = "";
  socksPass.value = "";
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
