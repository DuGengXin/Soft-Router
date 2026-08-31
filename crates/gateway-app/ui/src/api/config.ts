import { ref } from "vue";
import { api, ElMessage } from "./client";

export type Cfg = {
  mode: string;
  wan: { interface: string; address?: string; gateway?: string; dns: string[] };
  lan: { interface: string; address?: string; dns: string[] };
  dhcp: {
    enabled: boolean;
    range_start: string;
    range_end: string;
    lease_time: string;
    reservations?: { mac: string; ip: string; hostname?: string }[];
  };
  wireguard: {
    enabled: boolean;
    interface: string;
    address: string;
    listen_port: number;
    peer_endpoint: string;
    peer_allowed_ips: string;
  };
  routing: { china_direct: boolean; extra_direct_cidrs?: string[] };
  ui: { bind: string; port: number };
  port_forwards?: {
    enabled: boolean;
    protocol: string;
    wan_port: number;
    lan_ip: string;
    lan_port: number;
  }[];
};

export async function loadConfig(): Promise<Cfg> {
  return (await api("/api/v1/config")) as Cfg;
}

export async function saveConfig(cfg: Cfg) {
  await api("/api/v1/config", { method: "PUT", body: JSON.stringify(cfg) });
  ElMessage.success("配置已保存。尚未应用到数据面，请到「计划与变更」确认。");
}

export const nics = ref<string[]>([]);

export async function loadNics() {
  try {
    const st = (await api("/api/v1/status")) as { interfaces?: string[] };
    nics.value = st.interfaces || [];
  } catch {
    nics.value = [];
  }
}
