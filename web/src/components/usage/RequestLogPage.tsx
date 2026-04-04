import { useState } from 'react';
import { Tabs, TabsList, TabsTrigger } from '@/components/ui/tabs';
import { RequestLogTable } from './RequestLogTable';
import { useEndpoints } from '@/hooks/use-usage';
import type { TimeRange } from '@/types/usage';
import { Timer, RefreshCw } from 'lucide-react';

const REFRESH_OPTIONS = [
  { label: '关闭', value: 0 },
  { label: '5s', value: 5000 },
  { label: '10s', value: 10000 },
  { label: '30s', value: 30000 },
  { label: '60s', value: 60000 },
];

export function RequestLogPage() {
  const [timeRange, setTimeRange] = useState<TimeRange>('1h');
  const [refreshIdx, setRefreshIdx] = useState(3);
  const refreshMs = REFRESH_OPTIONS[refreshIdx].value;
  const { data: endpoints } = useEndpoints();

  function cycleRefresh() {
    setRefreshIdx((i) => (i + 1) % REFRESH_OPTIONS.length);
  }

  return (
    <div className="space-y-6">
      <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
        <div>
          <h2 className="text-[clamp(28px,4vw,38px)] font-semibold leading-[1.08] tracking-tight">
            请求日志
          </h2>
          <p className="mt-2 text-muted-foreground text-[15px] leading-relaxed">
            查看所有 API 请求的详细记录
          </p>
        </div>
        <div className="flex items-center gap-2">
          <button
            onClick={cycleRefresh}
            className="inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-background text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors"
          >
            {refreshMs > 0 ? <RefreshCw className="h-3.5 w-3.5 animate-spin" style={{ animationDuration: '3s' }} /> : <Timer className="h-3.5 w-3.5" />}
            {REFRESH_OPTIONS[refreshIdx].label}
          </button>
          <Tabs value={timeRange} onValueChange={(v) => setTimeRange(v as TimeRange)}>
            <TabsList>
              <TabsTrigger value="1h">1小时</TabsTrigger>
              <TabsTrigger value="6h">6小时</TabsTrigger>
              <TabsTrigger value="1d">24小时</TabsTrigger>
              <TabsTrigger value="7d">7天</TabsTrigger>
              <TabsTrigger value="30d">30天</TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
      </div>

      <RequestLogTable timeRange={timeRange} refreshMs={refreshMs} endpoints={endpoints} />
    </div>
  );
}
