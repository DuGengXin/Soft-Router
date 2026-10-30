import { ElMessage, ElMessageBox } from "element-plus";

export async function api(path: string, opts: RequestInit = {}) {
  const headers = new Headers(opts.headers);
  if (!headers.has("content-type")) {
    headers.set("content-type", "application/json");
  }
  const res = await fetch(path, { ...opts, headers, credentials: "same-origin" });
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
    const authEndpoint = path.startsWith("/api/v1/auth/");
    if (res.status === 401 && !authEndpoint) {
      window.dispatchEvent(new CustomEvent("gateway-auth-expired"));
    }
    if (!authEndpoint) ElMessage.error(msg);
    throw new Error(msg);
  }
  return data;
}

export { ElMessage, ElMessageBox };
