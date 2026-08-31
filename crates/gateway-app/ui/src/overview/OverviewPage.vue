<template>
  <div v-loading="loading">
    <el-row :gutter="16">
      <el-col :xs="24" :sm="12" :lg="6" v-for="g in gauges" :key="g.title">
        <el-card shadow="never" class="gauge">
          <div class="label">{{ g.title }}</div>
          <el-progress type="dashboard" :percentage="g.percent" :color="g.color" />
          <div class="sub">{{ g.sub }}</div>
        </el-card>
      </el-col>
    </el-row>
    <el-descriptions class="mt" :column="1" :direction="wide ? 'horizontal' : 'vertical'" border>
      <el-descriptions-item label="主机">{{ host.hostname }}</el-descriptions-item>
      <el-descriptions-item label="系统">{{ host.os }}</el-descriptions-item>
      <el-descriptions-item label="内核">{{ host.kernel }}</el-descriptions-item>
      <el-descriptions-item label="运行时间">{{ uptime }}</el-descriptions-item>
      <el-descriptions-item label="负载">{{ load }}</el-descriptions-item>
      <el-descriptions-item label="网卡流量">{{ netLine }}</el-descriptions-item>
    </el-descriptions>
    <el-row :gutter="16" class="mt">
      <el-col :xs="24" :sm="12" :lg="8" v-for="item in cards" :key="item.title">
        <el-card shadow="never">
          <div class="label">{{ item.title }}</div>
          <div class="value">{{ item.value }}</div>
          <div class="sub left">{{ item.sub }}</div>
        </el-card>
      </el-col>
    </el-row>
    <el-card class="mt" shadow="never">
      <template #header>冲突与建议</template>
      <el-table :data="status.conflicts || []" empty-text="没有发现冲突">
        <el-table-column prop="severity" label="级别" width="120">
          <template #default="{ row }">
            <el-tag :type="row.severity === 'blocker' ? 'danger' : 'warning'" size="small">
              {{ row.severity }}
            </el-tag>
          </template>
        </el-table-column>
        <el-table-column prop="title" label="标题" />
        <el-table-column prop="detail" label="说明" />
      </el-table>
    </el-card>
    <el-card class="mt" shadow="never">
      <template #header>最近事件</template>
      <el-table :data="events" empty-text="还没有事件">
        <el-table-column label="时间" width="180">
          <template #default="{ row }">
            {{ fmtTime(row.created_at) }}
          </template>
        </el-table-column>
        <el-table-column prop="kind" label="类型" width="160" />
        <el-table-column prop="payload" label="内容" />
      </el-table>
    </el-card>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, onUnmounted, reactive, ref } from "vue";
import { api } from "../api/client";

type Status = {
  mode?: string;
  blockers?: boolean;
  wan_uplink?: string;
  tunnel_uplink?: string;
  wan_direct_cidr?: string;
  conflicts?: { severity: string; title: string; detail: string }[];
  wireguard?: { enabled?: boolean; interface?: string };
  routing?: { china_direct?: boolean };
  dataplane?: { status?: string; message?: string; notes?: string[] };
};

type Host = {
  hostname?: string;
  os?: string;
  kernel?: string;
  uptime_secs?: number;
  cpu_percent?: number;
  cpu_count?: number;
  load_1?: number;
  load_5?: number;
  load_15?: number;
  mem_total_bytes?: number;
  mem_used_bytes?: number;
  disks?: { mount: string; used_bytes: number; total_bytes: number }[];
  nets?: { name: string; rx_bytes: number; tx_bytes: number }[];
};

const loading = ref(false);
const wide = ref(true);
const status = reactive<Status>({});
const host = reactive<Host>({});
const events = ref<{ created_at?: number; kind?: string; payload?: string }[]>([]);
let timer: number | undefined;

function pct(used?: number, total?: number) {
  if (!total) return 0;
  return Math.min(100, Math.round(((used || 0) / total) * 1000) / 10);
}
function color(p: number) {
  if (p >= 90) return "#f56c6c";
  if (p >= 75) return "#e6a23c";
  return "#3ecf8e";
}
function bytes(n?: number) {
  const u = ["B", "KB", "MB", "GB", "TB"];
  let i = 0;
  let v = Number(n) || 0;
  while (v >= 1024 && i < u.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v.toFixed(i === 0 ? 0 : 1)} ${u[i]}`;
}
function dataplaneLabel(dp?: Status["dataplane"]) {
  if (!dp?.status) return "未知";
  if (dp.status === "observe") return "未应用";
  if (dp.status === "healthy") return "健康";
  if (dp.status === "degraded") return "降级";
  return "异常";
}
function recoveryLabel(dp?: Status["dataplane"]) {
  if (status.mode !== "gateway") return "待应用";
  if (dp?.status === "healthy") return "运行中";
  if (dp?.status === "unhealthy" || dp?.status === "degraded") return "自愈中";
  return "已启用";
}
function linkLabel(s?: string) {
  if (s === "up") return "通";
  if (s === "down") return "断";
  if (s === "idle") return "未启用";
  return "未知";
}
function fmtTime(ts?: number) {
  return ts ? new Date(ts * 1000).toLocaleString() : "";
}

const rootDisk = computed(
  () => host.disks?.[0] || { used_bytes: 0, total_bytes: 0, mount: "-" }
);
const gauges = computed(() => {
  const cpu = Math.round((host.cpu_percent || 0) * 10) / 10;
  const mem = pct(host.mem_used_bytes, host.mem_total_bytes);
  const disk = pct(rootDisk.value.used_bytes, rootDisk.value.total_bytes);
  return [
    { title: "CPU", percent: cpu, color: color(cpu), sub: `${host.cpu_count || 0} 核` },
    {
      title: "内存",
      percent: mem,
      color: color(mem),
      sub: `${bytes(host.mem_used_bytes)} / ${bytes(host.mem_total_bytes)}`,
    },
    {
      title: "磁盘",
      percent: disk,
      color: color(disk),
      sub: `${rootDisk.value.mount} · ${bytes(rootDisk.value.used_bytes)} / ${bytes(rootDisk.value.total_bytes)}`,
    },
    {
      title: "负载",
      percent: Math.min(100, Math.round((host.load_1 || 0) * 10)),
      color: color((host.load_1 || 0) * 25),
      sub: `${(host.load_1 || 0).toFixed(2)} / ${(host.load_5 || 0).toFixed(2)} / ${(host.load_15 || 0).toFixed(2)}`,
    },
  ];
});
const uptime = computed(() => {
  const s = host.uptime_secs || 0;
  const d = Math.floor(s / 86400);
  const h = Math.floor((s % 86400) / 3600);
  const m = Math.floor((s % 3600) / 60);
  return `${d} 天 ${h} 小时 ${m} 分`;
});
const load = computed(
  () =>
    `${(host.load_1 || 0).toFixed(2)} / ${(host.load_5 || 0).toFixed(2)} / ${(host.load_15 || 0).toFixed(2)}`
);
const netLine = computed(
  () =>
    (host.nets || [])
      .slice(0, 4)
      .map((n) => `${n.name} ↓${bytes(n.rx_bytes)} ↑${bytes(n.tx_bytes)}`)
      .join(" · ") || "—"
);
const cards = computed(() => [
  {
    title: "上级链路",
    value: linkLabel(status.wan_uplink),
    sub: status.wan_direct_cidr ? `直连 ${status.wan_direct_cidr}` : "WAN 网关 ping",
  },
  {
    title: "隧道链路",
    value: linkLabel(status.tunnel_uplink),
    sub: "WireGuard handshake",
  },
  {
    title: "运行模式",
    value: status.mode === "gateway" ? "网关" : "观察",
    sub: "安装默认观察，确认后才改网络",
  },
  {
    title: "冲突阻断",
    value: status.blockers ? "是" : "否",
    sub: status.blockers ? "先处理冲突再应用" : "可以预览计划",
  },
  {
    title: "WireGuard",
    value: status.wireguard?.enabled ? "开" : "关",
    sub: status.wireguard?.interface || "wg0",
  },
  {
    title: "国内直连",
    value: status.routing?.china_direct ? "开" : "关",
    sub: "境外走隧道",
  },
  {
    title: "数据面",
    value: dataplaneLabel(status.dataplane),
    sub: status.dataplane?.message || (status.dataplane?.notes || []).join("；") || "未探测",
  },
  {
    title: "启动恢复",
    value: recoveryLabel(status.dataplane),
    sub:
      status.dataplane?.status === "unhealthy"
        ? (status.dataplane.notes || []).join("；") || "健康检查失败，连续失败后自动旁路"
        : "动态等待 LAN/WAN · 单元失败自动重启",
  },
]);

async function refresh() {
  try {
    Object.assign(status, await api("/api/v1/status"));
    Object.assign(host, await api("/api/v1/monitor"));
    const ev = await api("/api/v1/events");
    events.value = Array.isArray(ev) ? ev : [];
  } catch {
    /* 无后端时保持空态 */
  } finally {
    loading.value = false;
  }
}

onMounted(() => {
  wide.value = window.matchMedia("(min-width: 992px)").matches;
  loading.value = true;
  refresh();
  timer = window.setInterval(refresh, 5000);
});
onUnmounted(() => {
  if (timer) clearInterval(timer);
});
</script>

<style scoped>
.label {
  color: var(--el-text-color-secondary);
  font-size: 13px;
}
.value {
  font-size: 28px;
  font-weight: 700;
  margin: 8px 0 4px;
}
.sub {
  color: var(--el-text-color-secondary);
  font-size: 12px;
  text-align: center;
}
.sub.left {
  text-align: left;
}
.mt {
  margin-top: 16px;
}
.gauge {
  text-align: center;
}
.gauge .label {
  margin-bottom: 8px;
}
</style>
