import type { UsageSummary, DailyStats, PaginatedLogs, ModelStats, ModelPricing, ApiKey, Endpoint, TestKeyResult, AccessToken, AccessTokenCreated } from '@/types/usage';

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

  getUsageSummary: (hours: number, channelId?: string) => {
    const sp = new URLSearchParams({ hours: String(hours) });
    if (channelId) sp.set('channel_id', channelId);
    return fetchJson<UsageSummary>(`${BASE}/api/stats/summary?${sp}`);
  },

  getUsageTrends: (hours: number, channelId?: string) => {
    const sp = new URLSearchParams({ hours: String(hours) });
    if (channelId) sp.set('channel_id', channelId);
    return fetchJson<DailyStats[]>(`${BASE}/api/stats/trends?${sp}`);
  },

  getModelStats: (days: number) =>
    fetchJson<ModelStats[]>(`${BASE}/api/stats/models?days=${days}`),

  getRequestLogs: (params: {
    page?: number;
    page_size?: number;
    status_code?: number;
    model?: string;
    days?: number;
    channel_id?: string;
  }) => {
    const sp = new URLSearchParams();
    if (params.page) sp.set('page', String(params.page));
    if (params.page_size) sp.set('page_size', String(params.page_size));
    if (params.status_code) sp.set('status_code', String(params.status_code));
    if (params.model) sp.set('model', params.model);
    if (params.days) sp.set('days', String(params.days));
    if (params.channel_id) sp.set('channel_id', params.channel_id);
    return fetchJson<PaginatedLogs>(`${BASE}/api/stats/logs?${sp}`);
  },

  getModelPricing: () =>
    fetchJson<ModelPricing[]>(`${BASE}/api/stats/pricing`),

  getConfig: () =>
    fetchJson<Record<string, unknown>>(`${BASE}/api/config`),

  // 渠道 API Key
  listApiKeys: (endpointId?: string) => {
    const sp = endpointId ? `?endpoint_id=${endpointId}` : '';
    return fetchJson<ApiKey[]>(`/api/keys${sp}`);
  },
  addApiKey: (data: { endpoint_id: string; api_key: string; label: string }) => postJson<ApiKey>('/api/keys', data),
  deleteApiKey: (id: string) => deleteJson<{ ok: boolean }>(`/api/keys/${id}`),
  toggleApiKey: (id: string, is_active: boolean) => putJson<{ ok: boolean }>(`/api/keys/${id}/status`, { is_active }),
  getApiKeyFull: (id: string) => fetchJson<{ api_key: string }>(`/api/keys/${id}/full`),
  testApiKey: (id: string) => postJson<TestKeyResult>(`/api/keys/${id}/test`, {}),

  // 渠道（上游端点）
  listEndpoints: () => fetchJson<Endpoint[]>('/api/endpoints'),
  addEndpoint: (data: { name: string; base_url: string }) => postJson<Endpoint>('/api/endpoints', data),
  updateEndpoint: (id: string, data: { name: string; base_url: string }) => putJson<{ ok: boolean }>(`/api/endpoints/${id}`, data),
  deleteEndpoint: (id: string) => deleteJson<{ ok: boolean }>(`/api/endpoints/${id}`),
  toggleEndpoint: (id: string, is_active: boolean) => putJson<{ ok: boolean }>(`/api/endpoints/${id}/status`, { is_active }),

  // 访问密钥
  listAccessTokens: () => fetchJson<AccessToken[]>('/api/access-tokens'),
  addAccessToken: (data: { name: string; channel_ids: string[] }) => postJson<AccessTokenCreated>('/api/access-tokens', data),
  deleteAccessToken: (id: string) => deleteJson<{ ok: boolean }>(`/api/access-tokens/${id}`),
  toggleAccessToken: (id: string, is_active: boolean) => putJson<{ ok: boolean }>(`/api/access-tokens/${id}/status`, { is_active }),
  updateAccessTokenChannels: (id: string, channel_ids: string[]) => putJson<{ ok: boolean }>(`/api/access-tokens/${id}/channels`, { channel_ids }),
};
