<template>
  <el-card shadow="never" v-loading="loading">
    <template #header>端口映射 / DMZ 入口</template>
    <p class="hint">
      不改路由器 A。A 侧访问本网关 WAN IP 的端口，DNAT 到 LAN 设备。须网关模式并确认应用后生效。
    </p>
    <el-table v-if="form" :data="form.port_forwards || []">
      <el-table-column label="开" width="70">
        <template #default="{ row }">
          <el-switch v-model="row.enabled" />
        </template>
      </el-table-column>
      <el-table-column label="协议" width="110">
        <template #default="{ row }">
          <el-select v-model="row.protocol">
            <el-option label="tcp" value="tcp" />
            <el-option label="udp" value="udp" />
          </el-select>
        </template>
      </el-table-column>
      <el-table-column label="WAN 端口" width="120">
        <template #default="{ row }">
          <el-input-number v-model="row.wan_port" :min="1" :max="65535" />
        </template>
      </el-table-column>
      <el-table-column label="LAN IP">
        <template #default="{ row }">
          <el-input v-model="row.lan_ip" placeholder="192.168.50.10" />
        </template>
      </el-table-column>
      <el-table-column label="LAN 端口" width="120">
        <template #default="{ row }">
          <el-input-number v-model="row.lan_port" :min="1" :max="65535" />
        </template>
      </el-table-column>
      <el-table-column width="90">
        <template #default="{ $index }">
          <el-button type="danger" link @click="form.port_forwards.splice($index, 1)">删</el-button>
        </template>
      </el-table-column>
    </el-table>
    <el-button class="mt" @click="addFw">添加映射</el-button>
    <el-button type="primary" class="mt" @click="save">保存</el-button>
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
    if (!cfg.port_forwards) cfg.port_forwards = [];
    form.value = cfg;
  } finally {
    loading.value = false;
  }
});

function addFw() {
  form.value?.port_forwards?.push({
    enabled: true,
    protocol: "tcp",
    wan_port: 8080,
    lan_ip: "",
    lan_port: 80,
  });
}

async function save() {
  if (form.value) await saveConfig(form.value);
}
</script>

<style scoped>
.hint {
  color: var(--el-text-color-secondary);
  margin-bottom: 12px;
}
.mt {
  margin-top: 12px;
  margin-right: 8px;
}
</style>
