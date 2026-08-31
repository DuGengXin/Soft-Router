<template>
  <el-card shadow="never">
    <template #header>
      <div class="head">
        <span>变更计划</span>
        <div class="actions">
          <el-button @click="preview">生成预览</el-button>
          <el-button type="danger" @click="apply">确认应用</el-button>
          <el-button @click="rollback">回滚</el-button>
          <el-button type="warning" @click="bypass">紧急旁路</el-button>
        </div>
      </div>
    </template>
    <el-alert :title="explain" type="info" :closable="false" class="mb" />
    <el-table :data="actions" empty-text="无动作（观察模式或被阻断）">
      <el-table-column prop="id" label="动作" width="180" />
      <el-table-column prop="summary" label="说明" />
    </el-table>
    <el-input type="textarea" :rows="12" class="mt" readonly :model-value="raw" />
  </el-card>
</template>

<script setup lang="ts">
import { ref } from "vue";
import { useRouter } from "vue-router";
import { api, ElMessage, ElMessageBox } from "../api/client";

const router = useRouter();
const explain = ref("尚未预览。应用会写入 nft 表 gateway_kit，不会改主默认路由。");
const actions = ref<{ id?: string; summary?: string }[]>([]);
const raw = ref("");

function show(plan: {
  status?: string;
  explanation?: string;
  message?: string;
  actions?: { id?: string; summary?: string }[];
}) {
  explain.value = `${plan.status || ""} · ${plan.explanation || plan.message || ""}`;
  actions.value = plan.actions || [];
  raw.value = JSON.stringify(plan, null, 2);
}

async function preview() {
  show((await api("/api/v1/plan")) as Parameters<typeof show>[0]);
  ElMessage.info("已生成计划预览（未执行）");
}

async function apply() {
  await ElMessageBox.confirm(
    "确认应用？将写入 nft 表 gateway_kit 与生成配置，不修改主默认路由。",
    "确认应用"
  );
  const st = (await api("/api/v1/status")) as { mode?: string };
  if (st.mode !== "gateway") {
    ElMessage.error("仍是观察模式，确认应用不会改网络。请到「分流」选「网关」并保存。");
    await router.push({ name: "routing" });
    return;
  }
  const result = (await api("/api/v1/apply", {
    method: "POST",
    body: JSON.stringify({ confirm: true }),
  })) as { message?: string };
  ElMessage.success(result.message || "已确认应用。");
  show((await api("/api/v1/plan")) as Parameters<typeof show>[0]);
}

async function rollback() {
  const result = (await api("/api/v1/rollback", {
    method: "POST",
    body: JSON.stringify({ confirm: true }),
  })) as { message?: string };
  ElMessage.success(result.message || "已请求回滚。");
  show((await api("/api/v1/plan")) as Parameters<typeof show>[0]);
}

async function bypass() {
  const result = (await api("/api/v1/disable", {
    method: "POST",
    body: JSON.stringify({ confirm: true }),
  })) as { message?: string };
  ElMessage.warning(result.message || "已进入紧急旁路。");
  show((await api("/api/v1/plan")) as Parameters<typeof show>[0]);
}
</script>

<style scoped>
.head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  gap: 12px;
  flex-wrap: wrap;
}
.actions {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}
.mb {
  margin-bottom: 12px;
}
.mt {
  margin-top: 12px;
}
</style>
