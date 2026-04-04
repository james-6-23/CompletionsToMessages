import { useState, useEffect, useCallback } from 'react';
import { api } from '@/lib/api';
import type { UsageSummary, DailyStats, ModelStats, PaginatedLogs, ApiKey } from '@/types/usage';

export function useUsageSummary(days: number, refreshMs: number) {
  const [data, setData] = useState<UsageSummary | null>(null);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.getUsageSummary(days).then(setData).catch(console.error).finally(() => setLoading(false));
  }, [days]);

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

export function useUsageTrends(days: number, refreshMs: number) {
  const [data, setData] = useState<DailyStats[]>([]);
  const [loading, setLoading] = useState(true);

  const refresh = useCallback(() => {
    api.getUsageTrends(days).then(setData).catch(console.error).finally(() => setLoading(false));
  }, [days]);

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

export function useRequestLogs(params: { page: number; pageSize: number; statusCode?: number; model?: string; days: number }, refreshMs: number) {
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
      }).then(setData).catch(console.error).finally(() => setLoading(false));
    refresh();
    if (refreshMs > 0) {
      const timer = setInterval(refresh, refreshMs);
      return () => clearInterval(timer);
    }
  }, [params.page, params.pageSize, params.statusCode, params.model, params.days, refreshMs]);

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
