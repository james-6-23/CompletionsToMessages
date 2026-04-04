import {
  AreaChart, Area, XAxis, YAxis, CartesianGrid, Tooltip, ResponsiveContainer,
} from 'recharts';
import type { DailyStats, TimeRange } from '@/types/usage';
import { fmtTokenK, fmtUsd, parseFiniteNumber } from './format';

interface Props {
  data: DailyStats[];
  loading: boolean;
  timeRange: TimeRange;
}

interface ChartRow {
  date: string;
  input: number;
  output: number;
  cacheCreate: number;
  cacheRead: number;
  cost: number;
}

function transform(rows: DailyStats[]): ChartRow[] {
  return rows.map((r) => ({
    date: r.date,
    input: r.total_input_tokens,
    output: r.total_output_tokens,
    cacheCreate: r.total_cache_creation_tokens,
    cacheRead: r.total_cache_read_tokens,
    cost: parseFiniteNumber(r.total_cost) ?? 0,
  }));
}

function formatXAxis(value: string, timeRange: TimeRange): string {
  if (timeRange === '1d') {
    const parts = value.split(' ');
    return parts.length > 1 ? parts[1].slice(0, 5) : value;
  }
  const parts = value.split('-');
  return parts.length >= 3 ? `${parts[1]}/${parts[2]}` : value;
}

interface TooltipPayloadItem {
  name: string;
  value: number;
  color: string;
  dataKey: string;
}

interface CustomTooltipProps {
  active?: boolean;
  payload?: TooltipPayloadItem[];
  label?: string;
}

const SERIES_LABELS: Record<string, string> = {
  input: '输入',
  output: '输出',
  cacheCreate: '缓存创建',
  cacheRead: '缓存命中',
  cost: '成本',
};

function CustomTooltip({ active, payload, label }: CustomTooltipProps) {
  if (!active || !payload?.length) return null;
  return (
    <div className="rounded-xl border border-border/60 bg-card p-3 shadow-lg text-sm">
      <p className="mb-2 font-semibold text-foreground">{label}</p>
      {payload.map((entry) => (
        <div key={entry.dataKey} className="flex items-center gap-2 py-0.5">
          <span className="inline-block h-2 w-2 rounded-full shrink-0" style={{ backgroundColor: entry.color }} />
          <span className="text-muted-foreground">{SERIES_LABELS[entry.dataKey] ?? entry.name}:</span>
          <span className="font-medium text-foreground">
            {entry.dataKey === 'cost' ? fmtUsd(entry.value, 4) : fmtTokenK(entry.value)}
          </span>
        </div>
      ))}
    </div>
  );
}

export function UsageTrendChart({ data, loading, timeRange }: Props) {
  const chartData = transform(data);

  return (
    <div className="rounded-2xl border border-border/50 bg-card p-6 shadow-sm">
      <h3 className="mb-5 text-lg font-semibold">趋势图</h3>
      {loading ? (
        <div className="flex h-[350px] items-center justify-center">
          <div className="spinner" />
        </div>
      ) : chartData.length === 0 ? (
        <div className="flex h-[350px] items-center justify-center text-muted-foreground">暂无数据</div>
      ) : (
        <ResponsiveContainer width="100%" height={350}>
          <AreaChart data={chartData} margin={{ top: 10, right: 10, left: 0, bottom: 0 }}>
            <defs>
              <linearGradient id="gInput" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#3b82f6" stopOpacity={0.25} />
                <stop offset="95%" stopColor="#3b82f6" stopOpacity={0} />
              </linearGradient>
              <linearGradient id="gOutput" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#22c55e" stopOpacity={0.25} />
                <stop offset="95%" stopColor="#22c55e" stopOpacity={0} />
              </linearGradient>
              <linearGradient id="gCacheCreate" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#f97316" stopOpacity={0.25} />
                <stop offset="95%" stopColor="#f97316" stopOpacity={0} />
              </linearGradient>
              <linearGradient id="gCacheRead" x1="0" y1="0" x2="0" y2="1">
                <stop offset="5%" stopColor="#a855f7" stopOpacity={0.25} />
                <stop offset="95%" stopColor="#a855f7" stopOpacity={0} />
              </linearGradient>
            </defs>
            <CartesianGrid strokeDasharray="3 3" stroke="hsl(var(--color-border))" strokeOpacity={0.5} />
            <XAxis
              dataKey="date"
              tickFormatter={(v: string) => formatXAxis(v, timeRange)}
              tick={{ fontSize: 12, fill: 'hsl(var(--color-muted-foreground))' }}
              axisLine={false}
              tickLine={false}
            />
            <YAxis
              yAxisId="tokens"
              tickFormatter={(v: number) => fmtTokenK(v)}
              tick={{ fontSize: 12, fill: 'hsl(var(--color-muted-foreground))' }}
              axisLine={false}
              tickLine={false}
            />
            <YAxis
              yAxisId="cost"
              orientation="right"
              tickFormatter={(v: number) => `$${v.toFixed(2)}`}
              tick={{ fontSize: 12, fill: 'hsl(var(--color-muted-foreground))' }}
              axisLine={false}
              tickLine={false}
            />
            <Tooltip content={<CustomTooltip />} />
            <Area yAxisId="tokens" type="monotone" dataKey="input" stroke="#3b82f6" fill="url(#gInput)" strokeWidth={2} name="输入" />
            <Area yAxisId="tokens" type="monotone" dataKey="output" stroke="#22c55e" fill="url(#gOutput)" strokeWidth={2} name="输出" />
            <Area yAxisId="tokens" type="monotone" dataKey="cacheCreate" stroke="#f97316" fill="url(#gCacheCreate)" strokeWidth={2} name="缓存创建" />
            <Area yAxisId="tokens" type="monotone" dataKey="cacheRead" stroke="#a855f7" fill="url(#gCacheRead)" strokeWidth={2} name="缓存命中" />
            <Area yAxisId="cost" type="monotone" dataKey="cost" stroke="#f43f5e" fill="none" strokeWidth={2} strokeDasharray="5 5" name="成本" />
          </AreaChart>
        </ResponsiveContainer>
      )}
    </div>
  );
}
