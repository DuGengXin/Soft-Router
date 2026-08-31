import { ElMessage, ElMessageBox } from "element-plus";

const token = () => localStorage.getItem("gk_token") || "";

export async function api(path: string, opts: RequestInit = {}) {
  const headers = new Headers(opts.headers);
  if (!headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }
  const t = token();
  if (t) headers.set("x-gateway-token", t);
  const res = await fetch(path, { ...opts, headers });
  const text = await res.text();
  let data: Record<string, unknown> | unknown[] | { raw: string };
  try {
    data = JSON.parse(text) as Record<string, unknown>;
  } catch {
    data = { raw: text };
  }
  if (!res.ok) {
    const rec = data as { raw?: string; message?: string };
    const msg = rec.raw || rec.message || text || String(res.status);
    ElMessage.error(msg);
    throw new Error(msg);
  }
  return data;
}

export { ElMessage, ElMessageBox };
