<template>
  <el-container class="shell">
    <el-aside v-if="wide" width="240px" class="aside">
      <NavMenu v-model="active" />
    </el-aside>
    <el-container>
      <el-header class="header">
        <el-button v-if="!wide" text class="menu-btn" @click="drawer = true">
          <el-icon :size="20"><Menu /></el-icon>
        </el-button>
        <div class="titles">
          <div class="crumb">控制面</div>
          <strong>{{ title }}</strong>
        </div>
        <el-tag :type="modeTag" effect="dark" size="large">{{ modeLabel }}</el-tag>
      </el-header>
      <el-main class="main">
        <router-view />
      </el-main>
    </el-container>
    <el-drawer v-model="drawer" direction="ltr" size="240px" :with-header="false" class="nav-drawer">
      <NavMenu v-model="active" @select="drawer = false" />
    </el-drawer>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Menu } from "@element-plus/icons-vue";
import { api } from "./api/client";
import NavMenu from "./shell/NavMenu.vue";

const titles: Record<string, string> = {
  overview: "概览",
  interfaces: "接口",
  routing: "分流",
  tunnel: "隧道",
  clients: "接入设备",
  forwards: "端口映射",
  plan: "计划与变更",
  access: "访问令牌",
};

const route = useRoute();
const router = useRouter();
const drawer = ref(false);
const wide = ref(true);
const mode = ref("observe");
let mq: MediaQueryList | undefined;
let timer: number | undefined;

const active = computed({
  get: () => String(route.name || "overview"),
  set: (name: string) => {
    router.push({ name });
  },
});

const title = computed(() => titles[active.value] || "概览");
const modeLabel = computed(() => (mode.value === "gateway" ? "网关" : "观察"));
const modeTag = computed(() => (mode.value === "gateway" ? "success" : "primary"));

function onMq() {
  wide.value = mq?.matches ?? true;
}

async function loadBadge() {
  try {
    const st = (await api("/api/v1/status")) as { mode?: string };
    mode.value = st.mode || "observe";
  } catch {
    /* 本机无 agent 时顶栏保持观察 */
  }
}

onMounted(() => {
  mq = window.matchMedia("(min-width: 992px)");
  onMq();
  mq.addEventListener("change", onMq);
  loadBadge();
  timer = window.setInterval(loadBadge, 5000);
});
onUnmounted(() => {
  mq?.removeEventListener("change", onMq);
  if (timer) clearInterval(timer);
});
</script>

<style>
html,
body,
#app,
.shell {
  height: 100%;
  margin: 0;
}
html.dark {
  --el-color-primary: #3ecf8e;
  --el-color-primary-light-3: #6ed9a8;
  --el-color-primary-light-5: #8fe0ba;
  --el-color-primary-light-7: #b3ead0;
  --el-color-primary-light-8: #c6f0dc;
  --el-color-primary-light-9: #daf5e8;
  --el-color-primary-dark-2: #32a672;
}
.aside,
.nav-drawer .el-drawer__body {
  background: #1b1f24;
  padding: 0;
  display: flex;
  flex-direction: column;
  height: 100%;
}
.header {
  display: flex;
  align-items: center;
  gap: 12px;
  border-bottom: 1px solid var(--el-border-color);
}
.titles {
  flex: 1;
  min-width: 0;
}
.crumb {
  font-size: 12px;
  color: var(--el-text-color-secondary);
}
.main {
  background: var(--el-bg-color-page);
}
.menu-btn {
  margin-left: -8px;
}
</style>
