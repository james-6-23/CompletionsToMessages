import { useState } from 'react';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '@/components/ui/select';
import { useRequestLogs } from '@/hooks/use-usage';
import type { TimeRange, Endpoint } from '@/types/usage';
import { fmtInt, fmtUsd, fmtTimestamp, fmtDuration } from './format';
import { ChevronLeft, ChevronRight } from 'lucide-react';

interface Props {
  timeRange: TimeRange;
  refreshMs: number;
  endpoints: Endpoint[];
}

const PAGE_SIZE = 20;

export function RequestLogTable({ timeRange, refreshMs, endpoints }: Props) {
  const [page, setPage] = useState(1);
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [modelFilter, setModelFilter] = useState('');
  const [channelFilter, setChannelFilter] = useState<string>('all');

  const daysMap: Record<string, number> = { '1h': 1, '6h': 1, '1d': 1, '7d': 7, '30d': 30 };
  const days = daysMap[timeRange] ?? 1;
  const statusCode = statusFilter !== 'all' ? Number(statusFilter) : undefined;
  const model = modelFilter.trim() || undefined;
  const channelId = channelFilter !== 'all' ? channelFilter : undefined;

  const { data, loading } = useRequestLogs(
    { page, pageSize: PAGE_SIZE, statusCode, model, days, channelId },
    refreshMs,
  );

  const totalPages = data ? Math.ceil(data.total / PAGE_SIZE) : 0;

  function durationColor(ms: number): string {
    if (ms <= 5000) return 'text-emerald-500';
    if (ms <= 120000) return 'text-amber-500';
    return 'text-red-500';
  }

  function getChannelName(id: string): string {
    if (!id) return '-';
    return endpoints.find(e => e.id === id)?.name || id.slice(0, 8);
  }

  return (
    <div className="space-y-4">
      {/* 过滤器 */}
      <div className="flex flex-wrap items-center gap-3">
        <Select value={statusFilter} onValueChange={(v) => { setStatusFilter(v); setPage(1); }}>
          <SelectTrigger className="w-[140px]">
            <SelectValue placeholder="状态码" />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all">全部状态</SelectItem>
            <SelectItem value="200">200</SelectItem>
            <SelectItem value="400">400</SelectItem>
            <SelectItem value="429">429</SelectItem>
            <SelectItem value="500">500</SelectItem>
          </SelectContent>
        </Select>
        {endpoints.length > 0 && (
          <Select value={channelFilter} onValueChange={(v) => { setChannelFilter(v); setPage(1); }}>
            <SelectTrigger className="w-[160px]">
              <SelectValue placeholder="全部渠道" />
            </SelectTrigger>
            <SelectContent>
              <SelectItem value="all">全部渠道</SelectItem>
              {endpoints.map(ep => (
                <SelectItem key={ep.id} value={ep.id}>{ep.name}</SelectItem>
              ))}
            </SelectContent>
          </Select>
        )}
        <Input
          placeholder="筛选模型..."
          value={modelFilter}
          onChange={(e) => { setModelFilter(e.target.value); setPage(1); }}
          className="w-[200px]"
        />
      </div>

      {/* 表格 */}
      <div className="rounded-2xl border border-border/50 bg-card shadow-sm overflow-hidden">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>时间</TableHead>
              <TableHead>渠道</TableHead>
              <TableHead>模型</TableHead>
              <TableHead className="text-right">Input</TableHead>
              <TableHead className="text-right">Output</TableHead>
              <TableHead className="text-right">缓存读取</TableHead>
              <TableHead className="text-right">缓存创建</TableHead>
              <TableHead className="text-right">总成本</TableHead>
              <TableHead className="text-right">耗时</TableHead>
              <TableHead className="text-center">状态码</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {loading ? (
              <TableRow>
                <TableCell colSpan={10} className="text-center py-12 text-muted-foreground">
                  <div className="flex items-center justify-center gap-2">
                    <div className="spinner !w-5 !h-5 !border-2" />
                    加载中...
                  </div>
                </TableCell>
              </TableRow>
            ) : !data?.data.length ? (
              <TableRow>
                <TableCell colSpan={10} className="text-center py-12 text-muted-foreground">暂无数据</TableCell>
              </TableRow>
            ) : (
              data.data.map((log) => (
                <TableRow key={log.request_id}>
                  <TableCell className="whitespace-nowrap text-xs text-muted-foreground">{fmtTimestamp(log.created_at)}</TableCell>
                  <TableCell className="text-xs">
                    <span className="inline-flex items-center px-1.5 py-0.5 rounded bg-muted text-muted-foreground font-medium truncate max-w-[120px]">
                      {getChannelName(log.channel_id)}
                    </span>
                  </TableCell>
                  <TableCell className="font-mono text-xs">{log.model}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.input_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.output_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.cache_read_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.cache_creation_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtUsd(log.total_cost_usd, 4)}</TableCell>
                  <TableCell className={`text-right tabular-nums text-sm ${durationColor(log.latency_ms)}`}>{fmtDuration(log.latency_ms)}</TableCell>
                  <TableCell className="text-center">
                    <Badge variant={log.status_code === 200 ? 'default' : 'destructive'}
                      className={log.status_code === 200
                        ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20'
                        : 'bg-red-500/10 text-red-500 border-red-500/20'}>
                      {log.status_code}
                    </Badge>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </div>

      {/* 分页 */}
      {totalPages > 1 && (
        <div className="flex items-center justify-between">
          <p className="text-sm text-muted-foreground">
            共 {data?.total ?? 0} 条，第 {page}/{totalPages} 页
          </p>
          <div className="flex items-center gap-1">
            <Button variant="outline" size="icon" disabled={page <= 1} onClick={() => setPage((p) => p - 1)} className="h-8 w-8">
              <ChevronLeft className="h-4 w-4" />
            </Button>
            {Array.from({ length: Math.min(5, totalPages) }, (_, i) => {
              const start = Math.max(1, Math.min(page - 2, totalPages - 4));
              const p = start + i;
              if (p > totalPages) return null;
              return (
                <Button key={p} variant={p === page ? 'default' : 'outline'} size="sm" onClick={() => setPage(p)} className="h-8 w-8 p-0">
                  {p}
                </Button>
              );
            })}
            <Button variant="outline" size="icon" disabled={page >= totalPages} onClick={() => setPage((p) => p + 1)} className="h-8 w-8">
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
