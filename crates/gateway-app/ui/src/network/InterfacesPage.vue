<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>WAN / LAN 接口</template>
    <el-form v-if="form" :model="form" label-width="140px">
      <el-divider content-position="left">WAN · 接路由器 A</el-divider>
      <el-form-item label="网卡">
        <el-select v-model="form.wan.interface" filterable allow-create style="width: 100%">
          <el-option v-for="n in nics" :key="'w' + n" :label="n" :value="n" />
        </el-select>
      </el-form-item>
      <el-form-item label="地址">
        <el-input v-model="form.wan.address" placeholder="192.168.40.2/24" />
      </el-form-item>
      <el-form-item label="网关">
        <el-input v-model="form.wan.gateway" placeholder="192.168.40.1" />
      </el-form-item>
      <el-form-item label="上游 DNS">
        <el-input v-model="dnsText" placeholder="填写本部署可用的 DNS，多个地址用逗号分隔" />
        <div class="hint">
          网关模式必须显式配置；dnsmasq 只转发到本机 sing-box，由 sing-box 按直连/代理出口查询。
        </div>
      </el-form-item>
      <el-divider content-position="left">LAN · 接 AP / 工作 PC</el-divider>
      <el-form-item label="网卡">
        <el-select v-model="form.lan.interface" filterable allow-create style="width: 100%">
          <el-option v-for="n in nics" :key="'l' + n" :label="n" :value="n" />
        </el-select>
      </el-form-item>
      <el-form-item label="地址">
        <el-input v-model="form.lan.address" placeholder="192.168.50.1/24" />
      </el-form-item>
      <el-form-item>
        <el-button type="primary" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </el-card>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { type Cfg, loadConfig, loadNics, nics, saveConfig } from "../api/config";

const loading = ref(true);
const form = ref<Cfg | null>(null);
const dnsText = ref("");

onMounted(async () => {
  try {
    await loadNics();
    form.value = await loadConfig();
    dnsText.value = (form.value.wan.dns || form.value.lan.dns || []).join(", ");
  } finally {
    loading.value = false;
  }
});

async function save() {
  if (!form.value) return;
  form.value.wan.dns = dnsText.value
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
