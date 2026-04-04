import type { UsageSummary, DailyStats, PaginatedLogs, ModelStats, ModelPricing, ApiKey, TestKeyResult } from '@/types/usage';

const BASE = '';

// 存储在 sessionStorage 的管理密钥
function getAdminSecret(): string | null {
  return sessionStorage.getItem('admin_secret');
}

export function setAdminSecret(secret: string) {
  sessionStorage.setItem('admin_secret', secret);
}

export function clearAdminSecret() {
  sessionStorage.removeItem('admin_secret');
}

function authHeaders(): Record<string, string> {
  const secret = getAdminSecret();
  if (secret) return { 'x-admin-secret': secret };
  return {};
}

async function fetchJson<T>(url: string): Promise<T> {
  const res = await fetch(url, { headers: authHeaders() });
  if (res.status === 401) throw new Error('UNAUTHORIZED');
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

async function postJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(body),
  });
  if (res.status === 401) throw new Error('UNAUTHORIZED');
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

async function putJson<T>(url: string, body: unknown): Promise<T> {
  const res = await fetch(url, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json', ...authHeaders() },
    body: JSON.stringify(body),
  });
  if (res.status === 401) throw new Error('UNAUTHORIZED');
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

async function deleteJson<T>(url: string): Promise<T> {
  const res = await fetch(url, { method: 'DELETE', headers: authHeaders() });
  if (res.status === 401) throw new Error('UNAUTHORIZED');
  if (!res.ok) throw new Error(`API error: ${res.status}`);
  return res.json();
}

export const api = {
  // 登录验证（不需要 admin header）
  verifyAdmin: (secret: string) =>
    fetch(`${BASE}/api/admin/verify`, {
      method: 'POST',
      headers: { 'Content-Type': 'application/json' },
      body: JSON.stringify({ secret }),
    }).then(r => r.json() as Promise<{ valid: boolean; auth_required: boolean }>),

  getUsageSummary: (days: number) =>
    fetchJson<UsageSummary>(`${BASE}/api/stats/summary?days=${days}`),

  getUsageTrends: (days: number) =>
    fetchJson<DailyStats[]>(`${BASE}/api/stats/trends?days=${days}`),

  getModelStats: (days: number) =>
    fetchJson<ModelStats[]>(`${BASE}/api/stats/models?days=${days}`),

  getRequestLogs: (params: {
    page?: number;
    page_size?: number;
    status_code?: number;
    model?: string;
    days?: number;
  }) => {
    const sp = new URLSearchParams();
    if (params.page) sp.set('page', String(params.page));
    if (params.page_size) sp.set('page_size', String(params.page_size));
    if (params.status_code) sp.set('status_code', String(params.status_code));
    if (params.model) sp.set('model', params.model);
    if (params.days) sp.set('days', String(params.days));
    return fetchJson<PaginatedLogs>(`${BASE}/api/stats/logs?${sp}`);
  },

  getModelPricing: () =>
    fetchJson<ModelPricing[]>(`${BASE}/api/stats/pricing`),

  getConfig: () =>
    fetchJson<Record<string, unknown>>(`${BASE}/api/config`),

  listApiKeys: () => fetchJson<ApiKey[]>('/api/keys'),
  addApiKey: (data: { api_key: string; label: string }) => postJson<ApiKey>('/api/keys', data),
  deleteApiKey: (id: string) => deleteJson<{ ok: boolean }>(`/api/keys/${id}`),
  toggleApiKey: (id: string, is_active: boolean) => putJson<{ ok: boolean }>(`/api/keys/${id}/status`, { is_active }),
  testApiKey: (id: string) => postJson<TestKeyResult>(`/api/keys/${id}/test`, {}),

  // 上游 URL
  getUpstreamUrl: () => fetchJson<{ base_url: string }>('/api/upstream'),
  setUpstreamUrl: (base_url: string) => putJson<{ ok: boolean }>('/api/upstream', { base_url }),

  // Auth Token
  getAuthToken: () => fetchJson<{ has_token: boolean; token_masked: string | null }>('/api/auth-token'),
  setAuthToken: (token?: string) => postJson<{ ok: boolean; token: string }>('/api/auth-token', { token: token || '' }),
};
