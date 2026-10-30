import { createApp } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import ElementPlus from "element-plus";
import zhCn from "element-plus/es/locale/lang/zh-cn";
import "element-plus/dist/index.css";
import "element-plus/theme-chalk/dark/css-vars.css";
import App from "./App.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/overview" },
    { path: "/overview", name: "overview", component: () => import("./overview/OverviewPage.vue") },
    { path: "/wizard", redirect: "/network/interfaces" },
    { path: "/network/interfaces", name: "interfaces", component: () => import("./network/InterfacesPage.vue") },
    { path: "/network/routing", name: "routing", component: () => import("./network/RoutingPage.vue") },
    { path: "/network/tunnel", name: "tunnel", component: () => import("./network/TunnelPage.vue") },
    { path: "/network/clients", name: "clients", component: () => import("./network/ClientsPage.vue") },
    { path: "/network/forwards", name: "forwards", component: () => import("./network/ForwardsPage.vue") },
    { path: "/plan", name: "plan", component: () => import("./plan/PlanPage.vue") },
    { path: "/account", name: "account", component: () => import("./access/AccountPage.vue") },
  ],
});

document.documentElement.classList.add("dark");
createApp(App).use(router).use(ElementPlus, { locale: zhCn }).mount("#app");
