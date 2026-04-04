import { useState, useEffect } from 'react';
import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '@/components/ui/select';
import { useRequestLogs } from '@/hooks/use-usage';
import { api } from '@/lib/api';
import type { TimeRange, Endpoint } from '@/types/usage';
import { fmtInt, fmtUsd, fmtTimestamp, fmtDuration } from './format';
import { ChevronLeft, ChevronRight } from 'lucide-react';

/* ------------------------------------------------------------------ */
/*  模型徽章：Claude 系列使用品牌配色 + 网站 logo 图标                  */
/* ------------------------------------------------------------------ */

// 使用网站同款 favicon.ico 图标
function ClaudeIcon() {
  return (
    <img
      src="/favicon.ico"
      alt=""
      width={13}
      height={13}
      className="shrink-0"
      aria-hidden="true"
    />
  );
}

// Claude 模型名 → 颜色方案
const CLAUDE_MODEL_STYLES: { pattern: RegExp; bg: string; text: string; border: string }[] = [
  // Opus — 深紫
  { pattern: /opus/i, bg: 'bg-purple-500/10', text: 'text-purple-600 dark:text-purple-400', border: 'border-purple-500/20' },
  // Sonnet — 蓝
  { pattern: /sonnet/i, bg: 'bg-blue-500/10', text: 'text-blue-600 dark:text-blue-400', border: 'border-blue-500/20' },
  // Haiku — 青
  { pattern: /haiku/i, bg: 'bg-cyan-500/10', text: 'text-cyan-600 dark:text-cyan-400', border: 'border-cyan-500/20' },
  // 通用 Claude（兜底）— 品牌紫
  { pattern: /claude/i, bg: 'bg-violet-500/10', text: 'text-violet-600 dark:text-violet-400', border: 'border-violet-500/20' },
];

function ModelBadge({ model }: { model: string }) {
  if (!model) return <span className="text-muted-foreground">-</span>;

  const style = CLAUDE_MODEL_STYLES.find(s => s.pattern.test(model));

  if (style) {
    return (
      <span className={`inline-flex items-center gap-1.5 px-2 py-0.5 rounded-full border text-sm font-semibold font-mono ${style.bg} ${style.text} ${style.border}`}>
        <ClaudeIcon />
        {model}
      </span>
    );
  }

  // 非 Claude 模型
  return (
    <span className="inline-flex items-center font-mono text-sm font-semibold text-foreground">
      {model}
    </span>
  );
}

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

  // key 序号映射：{ keyId → 序号 }，按每个 endpoint 内的 created_at 排序
  const [keyIndexMap, setKeyIndexMap] = useState<Record<string, number>>({});
  useEffect(() => {
    api.listApiKeys().then(keys => {
      const grouped: Record<string, string[]> = {};
      for (const k of keys) {
        (grouped[k.endpoint_id] ||= []).push(k.id);
      }
      const map: Record<string, number> = {};
      for (const ids of Object.values(grouped)) {
        ids.forEach((id, i) => { map[id] = i + 1; });
      }
      setKeyIndexMap(map);
    }).catch(() => {});
  }, []);

  const hoursMap: Record<string, number> = { '1h': 1, '6h': 6, '1d': 24, '7d': 168, '30d': 720 };
  const hours = hoursMap[timeRange] ?? 24;
  const statusCode = statusFilter !== 'all' ? Number(statusFilter) : undefined;
  const model = modelFilter.trim() || undefined;
  const channelId = channelFilter !== 'all' ? channelFilter : undefined;

  const { data, loading } = useRequestLogs(
    { page, pageSize: PAGE_SIZE, statusCode, model, hours, channelId },
    refreshMs,
  );

  const totalPages = data ? Math.ceil(data.total / PAGE_SIZE) : 0;

  function durationColor(ms: number): string {
    if (ms <= 5000) return 'text-emerald-600 dark:text-emerald-400';
    if (ms <= 120000) return 'text-amber-600 dark:text-amber-400';
    return 'text-red-500';
  }

  function durationBg(ms: number): string {
    if (ms <= 5000) return 'hsl(142 71% 45% / 0.1)';
    if (ms <= 120000) return 'hsl(38 92% 50% / 0.1)';
    return 'hsl(0 84% 60% / 0.1)';
  }

  // 渠道颜色方案（根据名称哈希分配）
  const CHANNEL_COLORS = [
    { bg: 'bg-blue-500/10', text: 'text-blue-700 dark:text-blue-400', border: 'border-blue-500/20' },
    { bg: 'bg-emerald-500/10', text: 'text-emerald-700 dark:text-emerald-400', border: 'border-emerald-500/20' },
    { bg: 'bg-violet-500/10', text: 'text-violet-700 dark:text-violet-400', border: 'border-violet-500/20' },
    { bg: 'bg-amber-500/10', text: 'text-amber-700 dark:text-amber-400', border: 'border-amber-500/20' },
    { bg: 'bg-rose-500/10', text: 'text-rose-700 dark:text-rose-400', border: 'border-rose-500/20' },
    { bg: 'bg-cyan-500/10', text: 'text-cyan-700 dark:text-cyan-400', border: 'border-cyan-500/20' },
    { bg: 'bg-orange-500/10', text: 'text-orange-700 dark:text-orange-400', border: 'border-orange-500/20' },
    { bg: 'bg-pink-500/10', text: 'text-pink-700 dark:text-pink-400', border: 'border-pink-500/20' },
    { bg: 'bg-indigo-500/10', text: 'text-indigo-700 dark:text-indigo-400', border: 'border-indigo-500/20' },
    { bg: 'bg-teal-500/10', text: 'text-teal-700 dark:text-teal-400', border: 'border-teal-500/20' },
  ];

  function getChannelInfo(id: string) {
    if (!id) return { name: '-', color: CHANNEL_COLORS[0], logoUrl: '' };
    const ep = endpoints.find(e => e.id === id);
    const name = ep?.name || id.slice(0, 8);
    let hash = 0;
    for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
    const color = CHANNEL_COLORS[Math.abs(hash) % CHANNEL_COLORS.length];
    const logoUrl = ep?.logo_url || '';
    return { name, color, logoUrl };
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
              <TableHead className="text-center">用时/首字</TableHead>
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
                    {(() => {
                      const ch = getChannelInfo(log.channel_id);
                      return (
                        <span className="inline-flex items-center gap-1">
                          <span className={`inline-flex items-center gap-1 px-1.5 py-0.5 rounded border font-semibold truncate max-w-[140px] ${ch.color.bg} ${ch.color.text} ${ch.color.border}`}>
                            {ch.logoUrl && (
                              <img src={ch.logoUrl} alt="" className="h-3.5 w-3.5 object-contain rounded-sm shrink-0" />
                            )}
                            {ch.name}
                          </span>
                          {log.key_id && keyIndexMap[log.key_id] && (
                            <span className="inline-flex items-center px-1 py-0.5 rounded bg-blue-500/10 text-blue-600 dark:text-blue-400 font-mono text-[10px] font-bold">
                              #{keyIndexMap[log.key_id]}
                            </span>
                          )}
                        </span>
                      );
                    })()}
                  </TableCell>
                  <TableCell><ModelBadge model={log.model} /></TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.input_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.output_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.cache_read_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtInt(log.cache_creation_tokens)}</TableCell>
                  <TableCell className="text-right tabular-nums text-sm">{fmtUsd(log.total_cost_usd, 4)}</TableCell>
                  <TableCell className="text-center">
                    <div className="inline-flex items-center gap-1">
                      <span className={`inline-flex items-center px-1.5 py-0.5 rounded-full text-xs font-medium ${durationColor(log.latency_ms)} bg-current/10`}
                        style={{ backgroundColor: durationBg(log.latency_ms) }}>
                        {fmtDuration(log.latency_ms)}
                      </span>
                      {log.first_token_ms != null && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded-full text-xs font-medium text-blue-600 dark:text-blue-400"
                          style={{ backgroundColor: 'hsl(217 91% 60% / 0.1)' }}>
                          {fmtDuration(log.first_token_ms)}
                        </span>
                      )}
                      {log.is_streaming && (
                        <span className="inline-flex items-center px-1.5 py-0.5 rounded-full text-xs font-medium text-amber-600 dark:text-amber-400"
                          style={{ backgroundColor: 'hsl(38 92% 50% / 0.1)' }}>
                          流
                        </span>
                      )}
                    </div>
                  </TableCell>
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
