<template>
  <div class="nav-root">
    <div class="brand">
      <span class="mark">GK</span>
      Gateway-Kit
    </div>
    <el-menu
      :key="modelValue"
      :default-active="modelValue"
      :default-openeds="['net']"
      background-color="#1b1f24"
      text-color="#cfd3dc"
      active-text-color="#3ecf8e"
      @select="onSelect"
    >
      <el-menu-item index="overview">概览</el-menu-item>
      <el-sub-menu index="net">
        <template #title>网络</template>
        <el-menu-item index="interfaces">接口</el-menu-item>
        <el-menu-item index="routing">分流</el-menu-item>
        <el-menu-item index="tunnel">隧道</el-menu-item>
        <el-menu-item index="clients">接入设备</el-menu-item>
        <el-menu-item index="forwards">端口映射</el-menu-item>
      </el-sub-menu>
      <el-menu-item index="plan">计划与变更</el-menu-item>
      <el-menu-item index="access">访问令牌</el-menu-item>
    </el-menu>
    <div class="foot">Element Plus · 与 1Panel 同系</div>
  </div>
</template>

<script setup lang="ts">
defineProps<{ modelValue: string }>();
const emit = defineEmits<{
  (e: "update:modelValue", v: string): void;
  (e: "select"): void;
}>();

function onSelect(name: string) {
  if (name === "net") return;
  emit("update:modelValue", name);
  emit("select");
}
</script>

<style scoped>
.nav-root {
  display: flex;
  flex-direction: column;
  height: 100%;
}
.brand {
  display: flex;
  align-items: center;
  gap: 10px;
  padding: 18px 16px 12px;
  font-weight: 700;
  color: #fff;
}
.mark {
  width: 28px;
  height: 28px;
  border-radius: 6px;
  background: #3ecf8e;
  color: #0b1210;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  font-size: 12px;
  font-weight: 800;
}
.foot {
  margin-top: auto;
  padding: 12px 16px 16px;
  font-size: 12px;
  color: #8b919a;
}
</style>
