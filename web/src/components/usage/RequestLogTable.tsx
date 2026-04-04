import { useState } from 'react';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '@/components/ui/select';
import { useRequestLogs } from '@/hooks/use-usage';
import type { TimeRange } from '@/types/usage';
import { fmtInt, fmtUsd, fmtTimestamp, fmtDuration } from './format';
import { ChevronLeft, ChevronRight } from 'lucide-react';

interface Props {
  timeRange: TimeRange;
  refreshMs: number;
}

const PAGE_SIZE = 20;

export function RequestLogTable({ timeRange, refreshMs }: Props) {
  const [page, setPage] = useState(1);
  const [statusFilter, setStatusFilter] = useState<string>('all');
  const [modelFilter, setModelFilter] = useState('');

  const days = timeRange === '1d' ? 1 : 7;
  const statusCode = statusFilter !== 'all' ? Number(statusFilter) : undefined;
  const model = modelFilter.trim() || undefined;

  const { data, loading } = useRequestLogs(
    { page, pageSize: PAGE_SIZE, statusCode, model, days },
    refreshMs,
  );

  const totalPages = data ? Math.ceil(data.total / PAGE_SIZE) : 0;

  function durationColor(ms: number): string {
    if (ms <= 5000) return 'text-green-500';
    if (ms <= 120000) return 'text-orange-500';
    return 'text-red-500';
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
        <Input
          placeholder="筛选模型..."
          value={modelFilter}
          onChange={(e) => { setModelFilter(e.target.value); setPage(1); }}
          className="w-[200px]"
        />
      </div>

      {/* 表格 */}
      <div className="rounded-xl border border-border/50 bg-card/80 backdrop-blur-sm overflow-hidden">
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>时间</TableHead>
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
                <TableCell colSpan={9} className="text-center py-8 text-muted-foreground">加载中...</TableCell>
              </TableRow>
            ) : !data?.data.length ? (
              <TableRow>
                <TableCell colSpan={9} className="text-center py-8 text-muted-foreground">暂无数据</TableCell>
              </TableRow>
            ) : (
              data.data.map((log) => (
                <TableRow key={log.request_id}>
                  <TableCell className="whitespace-nowrap text-xs">{fmtTimestamp(log.created_at)}</TableCell>
                  <TableCell className="font-mono text-xs">{log.model}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtInt(log.input_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtInt(log.output_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtInt(log.cache_read_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtInt(log.cache_creation_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums">{fmtUsd(log.total_cost_usd, 4)}</TableCell>
                  <TableCell className={`text-right tabular-nums ${durationColor(log.latency_ms)}`}>{fmtDuration(log.latency_ms)}</TableCell>
                  <TableCell className="text-center">
                    <Badge variant={log.status_code === 200 ? 'default' : 'destructive'}
                      className={log.status_code === 200
                        ? 'bg-green-500/10 text-green-500 border-green-500/20'
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
            <Button variant="outline" size="icon" disabled={page <= 1} onClick={() => setPage((p) => p - 1)}>
              <ChevronLeft className="h-4 w-4" />
            </Button>
            {Array.from({ length: Math.min(5, totalPages) }, (_, i) => {
              const start = Math.max(1, Math.min(page - 2, totalPages - 4));
              const p = start + i;
              if (p > totalPages) return null;
              return (
                <Button key={p} variant={p === page ? 'default' : 'outline'} size="sm" onClick={() => setPage(p)}>
                  {p}
                </Button>
              );
            })}
            <Button variant="outline" size="icon" disabled={page >= totalPages} onClick={() => setPage((p) => p + 1)}>
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
