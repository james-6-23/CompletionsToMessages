import { useState, useEffect, useCallback } from 'react';
import { api } from '@/lib/api';
import type { UsageSummary, DailyStats, ModelStats, PaginatedLogs, ApiKey, Endpoint } from '@/types/usage';

export function useUsageSummary(hours: number, refreshMs: number, channelId?: string) {
  const [data, setData] = useState<UsageSummary | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.getUsageSummary(hours, channelId).then(setData).catch(console.error).finally(() => setLoading(false));
  }, [hours, channelId]);

  useEffect(() => {
    setLoading(true);
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [refresh, refreshMs]);

  return { data, loading, refresh };
}

export function useUsageTrends(hours: number, refreshMs: number, channelId?: string) {
  const [data, setData] = useState<DailyStats[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.getUsageTrends(hours, channelId).then(setData).catch(console.error).finally(() => setLoading(false));
  }, [hours, channelId]);

  useEffect(() => {
    setLoading(true);
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [refresh, refreshMs]);

  return { data, loading, refresh };
}

export function useModelStats(days: number, refreshMs: number) {
  const [data, setData] = useState<ModelStats[]>([]);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    const refresh = () => api.getModelStats(days).then(setData).catch(console.error).finally(() => setLoading(false));
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [days, refreshMs]);

  return { data, loading };
}

export function useRequestLogs(params: { page: number; pageSize: number; statusCode?: number; model?: string; days?: number; hours?: number; channelId?: string }, refreshMs: number) {
  const [data, setData] = useState<PaginatedLogs | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    setLoading(true);
    const refresh = () =>
      api.getRequestLogs({
        page: params.page,
        page_size: params.pageSize,
        status_code: params.statusCode,
        model: params.model,
        days: params.days,
        hours: params.hours,
        channel_id: params.channelId,
      }).then(setData).catch(console.error).finally(() => setLoading(false));
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [params.page, params.pageSize, params.statusCode, params.model, params.days, params.hours, params.channelId, refreshMs]);

  return { data, loading };
}

export function useApiKeys(refreshMs: number) {
  const [data, setData] = useState<ApiKey[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.listApiKeys().then(setData).catch(console.error).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [refresh, refreshMs]);

  return { data, loading, refresh };
}

export function useEndpoints() {
  const [data, setData] = useState<Endpoint[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.listEndpoints().then(setData).catch(console.error).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
  }, [refresh]);

  return { data, loading, refresh };
}

/** 实时 RPM：每隔 refreshMs 查询最近 1 分钟的请求数 */
export function useRpm(refreshMs: number, channelId?: string) {
  const [rpm, setRpm] = useState<number | null>(null);

  const refresh = useCallback(() => {
    api.getUsageSummary(1, channelId, 1)
      .then(d => setRpm(d.total_requests))
      .catch(console.error);
  }, [channelId]);

  useEffect(() => {
    refresh();
    const interval = refreshMs > 0 ? refreshMs : 5000;
    const timer = setInterval(refresh, interval);
    return () => clearInterval(timer);
  }, [refresh, refreshMs]);

  return rpm;
}
