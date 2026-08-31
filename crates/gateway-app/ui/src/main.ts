import { createApp } from "vue";
import { createRouter, createWebHashHistory } from "vue-router";
import ElementPlus from "element-plus";
import zhCn from "element-plus/es/locale/lang/zh-cn";
import "element-plus/dist/index.css";
import "element-plus/theme-chalk/dark/css-vars.css";
import App from "./App.vue";
import OverviewPage from "./overview/OverviewPage.vue";
import InterfacesPage from "./network/InterfacesPage.vue";
import RoutingPage from "./network/RoutingPage.vue";
import TunnelPage from "./network/TunnelPage.vue";
import ClientsPage from "./network/ClientsPage.vue";
import ForwardsPage from "./network/ForwardsPage.vue";
import PlanPage from "./plan/PlanPage.vue";
import AccessPage from "./access/AccessPage.vue";

const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    { path: "/", redirect: "/overview" },
    { path: "/overview", name: "overview", component: OverviewPage },
    { path: "/wizard", redirect: "/network/interfaces" },
    { path: "/network/interfaces", name: "interfaces", component: InterfacesPage },
    { path: "/network/routing", name: "routing", component: RoutingPage },
    { path: "/network/tunnel", name: "tunnel", component: TunnelPage },
    { path: "/network/clients", name: "clients", component: ClientsPage },
    { path: "/network/forwards", name: "forwards", component: ForwardsPage },
    { path: "/plan", name: "plan", component: PlanPage },
    { path: "/access", name: "access", component: AccessPage },
  ],
});

createApp(App).use(router).use(ElementPlus, { locale: zhCn }).mount("#app");
