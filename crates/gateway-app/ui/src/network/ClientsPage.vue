<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>DHCP 与接入设备</template>
    <el-form v-if="form" label-width="140px">
      <el-form-item label="启用 DHCP">
        <el-switch v-model="form.dhcp.enabled" />
      </el-form-item>
      <el-form-item label="地址池起始">
        <el-input v-model="form.dhcp.range_start" />
      </el-form-item>
      <el-form-item label="地址池结束">
        <el-input v-model="form.dhcp.range_end" />
      </el-form-item>
      <el-form-item label="租期">
        <el-input v-model="form.dhcp.lease_time" placeholder="12h" />
      </el-form-item>
      <el-divider content-position="left">MAC 固定 IP</el-divider>
      <p class="hint">限速将在后续版本用 nft/tc 落地。当前只写 dnsmasq dhcp-host。</p>
      <el-table :data="form.dhcp.reservations || []">
        <el-table-column label="MAC">
          <template #default="{ row }">
            <el-input v-model="row.mac" placeholder="aa:bb:cc:dd:ee:ff" />
          </template>
        </el-table-column>
        <el-table-column label="IP">
          <template #default="{ row }">
            <el-input v-model="row.ip" placeholder="192.168.50.20" />
          </template>
        </el-table-column>
        <el-table-column label="名称">
          <template #default="{ row }">
            <el-input v-model="row.hostname" />
          </template>
        </el-table-column>
        <el-table-column width="90">
          <template #default="{ $index }">
            <el-button type="danger" link @click="form.dhcp.reservations.splice($index, 1)">删</el-button>
          </template>
        </el-table-column>
      </el-table>
      <el-button class="mt" @click="addRes">添加绑定</el-button>
      <el-form-item class="mt">
        <el-button type="primary" @click="save">保存</el-button>
      </el-form-item>
    </el-form>
  </el-card>
</template>

<script setup lang="ts">
import { onMounted, ref } from "vue";
import { type Cfg, loadConfig, saveConfig } from "../api/config";

const loading = ref(true);
const form = ref<Cfg | null>(null);

onMounted(async () => {
  try {
    const cfg = await loadConfig();
    if (!cfg.dhcp.reservations) cfg.dhcp.reservations = [];
    form.value = cfg;
  } finally {
    loading.value = false;
  }
});

function addRes() {
  form.value?.dhcp.reservations?.push({ mac: "", ip: "", hostname: "" });
}

async function save() {
  if (form.value) await saveConfig(form.value);
}
</script>

<style scoped>
.hint {
  color: var(--el-text-color-secondary);
  font-size: 12px;
}
.mt {
  margin-top: 12px;
}
</style>
