<template>
  <div v-if="authLoading" class="auth-loading">正在检查管理员会话…</div>
  <AuthPage
    v-else-if="!authenticated || passwordChangeRequired"
    :must-change="passwordChangeRequired"
    :username="username"
    @authenticated="onAuthenticated"
  />
  <el-container v-else class="shell">
      <el-aside v-if="wide" width="240px" class="aside"><NavMenu v-model="active" /></el-aside>
      <el-container>
        <el-header class="header">
          <el-button v-if="!wide" text class="menu-btn" @click="drawer = true"><el-icon :size="20"><Menu /></el-icon></el-button>
          <div class="titles"><div class="crumb">GATEWAY-KIT / CONTROL</div><strong>{{ title }}</strong></div>
          <el-tag :type="modeTag" effect="dark" size="large">{{ modeLabel }}</el-tag>
          <el-divider direction="vertical" />
          <span class="user-name">{{ username }}</span>
          <el-button text @click="logout">退出</el-button>
        </el-header>
        <el-main class="main">
          <router-view v-slot="{ Component }">
            <component :is="Component" :username="username" @updated="username = $event" />
          </router-view>
        </el-main>
      </el-container>
      <el-drawer v-model="drawer" direction="ltr" size="240px" :with-header="false" class="nav-drawer">
        <NavMenu v-model="active" @select="drawer = false" />
      </el-drawer>
  </el-container>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { useRoute, useRouter } from "vue-router";
import { Menu } from "@element-plus/icons-vue";
import { api } from "./api/client";
import AuthPage from "./auth/AuthPage.vue";
import NavMenu from "./shell/NavMenu.vue";

const titles: Record<string, string> = {
  overview: "概览",
  interfaces: "接口",
  routing: "分流",
  tunnel: "隧道",
  clients: "接入设备",
  forwards: "端口映射",
  plan: "计划与变更",
  account: "账号与安全",
};

const route = useRoute();
const router = useRouter();
const drawer = ref(false);
const wide = ref(true);
const mode = ref("observe");
const authLoading = ref(true);
const authenticated = ref(false);
const passwordChangeRequired = ref(false);
const username = ref("");
let mq: MediaQueryList | undefined;
let timer: number | undefined;
const authExpiredEvent = () => {
  authenticated.value = false;
  passwordChangeRequired.value = false;
  username.value = "";
};

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

async function checkSession() {
  try {
    const result = await api("/api/v1/auth/session") as {
      authenticated?: boolean; username?: string; password_change_required?: boolean;
    };
    authenticated.value = Boolean(result.authenticated);
    username.value = result.username || "";
    passwordChangeRequired.value = Boolean(result.password_change_required);
  } catch {
    authenticated.value = false;
  } finally {
    authLoading.value = false;
  }
}

function onAuthenticated(result: { username: string; password_change_required: boolean }) {
  authenticated.value = true;
  username.value = result.username;
  passwordChangeRequired.value = result.password_change_required;
  if (!result.password_change_required) loadBadge();
}

async function logout() {
  try { await api("/api/v1/auth/logout", { method: "POST" }); } catch { /* clear local shell anyway */ }
  authExpiredEvent();
}

function startPolling() {
  if (timer) return;
  loadBadge();
  timer = window.setInterval(loadBadge, 5000);
}

function stopPolling() {
  if (timer) clearInterval(timer);
  timer = undefined;
}

onMounted(() => {
  mq = window.matchMedia("(min-width: 992px)");
  onMq();
  mq.addEventListener("change", onMq);
  window.addEventListener("gateway-auth-expired", authExpiredEvent);
  checkSession();
});
watch([authenticated, passwordChangeRequired], ([value, mustChange]) =>
  value && !mustChange ? startPolling() : stopPolling()
);
onUnmounted(() => {
  mq?.removeEventListener("change", onMq);
  window.removeEventListener("gateway-auth-expired", authExpiredEvent);
  stopPolling();
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
.user-name { color: var(--el-text-color-secondary); font-size: 13px; }
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
.auth-loading { min-height: 100vh; display: grid; place-items: center; background: #11191d; color: #9baaa6; font: 13px ui-monospace, monospace; }
</style>
