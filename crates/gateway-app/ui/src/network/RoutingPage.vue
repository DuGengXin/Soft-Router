<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>分流</template>
    <el-form v-if="form" :model="form" label-width="160px">
      <el-form-item label="运行模式">
        <el-select v-model="form.mode" style="max-width: 320px" @change="onMode">
          <el-option label="观察（不改网络）" value="observe" />
          <el-option label="网关（需确认应用）" value="gateway" />
        </el-select>
        <div class="hint">网关模式保存后须到「计划与变更」确认应用。</div>
      </el-form-item>
      <el-form-item label="国内直连">
        <el-checkbox v-model="form.routing.china_direct">国内走 WAN，境外走隧道</el-checkbox>
      </el-form-item>
      <el-form-item label="上级局域网">
        <el-input :model-value="wanDirect" disabled />
        <div class="hint">
          由 WAN 地址算出，始终直连并 MASQUERADE。工作电脑可访问路由器 A 一侧，无需在 A 上加路由。
        </div>
      </el-form-item>
      <el-form-item label="额外直连 CIDR">
        <el-input v-model="extraDirect" type="textarea" :rows="2" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </el-card>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from "vue";
import { type Cfg, loadConfig, saveConfig } from "../api/config";

const loading = ref(true);
const form = ref<Cfg | null>(null);
const extraDirect = ref("");
const wanDirect = computed(() => {
  const addr = form.value?.wan.address || "";
  const [ip, pfx] = addr.split("/");
  if (!ip || !pfx) return "填写 WAN 地址后自动显示";
  const parts = ip.split(".").map((x) => Number(x));
  const p = Number(pfx);
  if (parts.length !== 4 || !Number.isFinite(p)) return addr;
  const mask = p === 0 ? 0 : (0xffffffff << (32 - p)) >>> 0;
  const n = ((parts[0] << 24) | (parts[1] << 16) | (parts[2] << 8) | parts[3]) >>> 0;
  const net = n & mask;
  return `${(net >>> 24) & 255}.${(net >>> 16) & 255}.${(net >>> 8) & 255}.${net & 255}/${p}`;
});

function onMode(mode: string) {
  if (form.value && mode === "gateway") form.value.wireguard.enabled = true;
}

onMounted(async () => {
  try {
    form.value = await loadConfig();
    extraDirect.value = (form.value.routing.extra_direct_cidrs || []).join(", ");
  } finally {
    loading.value = false;
  }
});

async function save() {
  if (!form.value) return;
  form.value.routing.extra_direct_cidrs = extraDirect.value
    .split(/[\s,]+/)
    .map((s) => s.trim())
    .filter(Boolean);
  await saveConfig(form.value);
}
</script>

<style scoped>
.hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  margin-top: 6px;
  line-height: 1.5;
}
</style>
