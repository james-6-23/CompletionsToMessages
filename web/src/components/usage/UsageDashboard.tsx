import { useState } from 'react';
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';
import { Button } from '@/components/ui/button';
import { UsageSummaryCards } from './UsageSummaryCards';
import { UsageTrendChart } from './UsageTrendChart';
import { RequestLogTable } from './RequestLogTable';
import { ModelStatsTable } from './ModelStatsTable';
import { ApiKeyManager } from './ApiKeyManager';
import { useUsageSummary, useUsageTrends, useModelStats } from '@/hooks/use-usage';
import type { TimeRange } from '@/types/usage';
import { Timer, KeyRound } from 'lucide-react';

const REFRESH_OPTIONS = [
  { label: '关闭', value: 0 },
  { label: '5s', value: 5000 },
  { label: '10s', value: 10000 },
  { label: '30s', value: 30000 },
  { label: '60s', value: 60000 },
];

export function UsageDashboard() {
  const [timeRange, setTimeRange] = useState<TimeRange>('1d');
  const [refreshIdx, setRefreshIdx] = useState(3); // 默认 30s
  const refreshMs = REFRESH_OPTIONS[refreshIdx].value;
  const days = timeRange === '1d' ? 1 : 7;

  const summary = useUsageSummary(days, refreshMs);
  const trends = useUsageTrends(days, refreshMs);
  const models = useModelStats(days, refreshMs);

  function cycleRefresh() {
    setRefreshIdx((i) => (i + 1) % REFRESH_OPTIONS.length);
  }

  return (
    <div className="space-y-6">
      {/* 页头 */}
      <div className="flex flex-col gap-4 sm:flex-row sm:items-center sm:justify-between">
        <div>
          <h1 className="text-3xl font-bold tracking-tight">使用统计</h1>
          <p className="mt-1 text-muted-foreground">查看 AI 模型的使用情况和成本统计</p>
        </div>
        <div className="flex items-center gap-2">
          {/* 刷新间隔 */}
          <Button variant="outline" size="sm" onClick={cycleRefresh} className="gap-1.5">
            <Timer className="h-4 w-4" />
            {REFRESH_OPTIONS[refreshIdx].label}
          </Button>
          {/* 时间范围 */}
          <Tabs value={timeRange} onValueChange={(v) => setTimeRange(v as TimeRange)}>
            <TabsList>
              <TabsTrigger value="1d">24小时</TabsTrigger>
              <TabsTrigger value="7d">7天</TabsTrigger>
            </TabsList>
          </Tabs>
        </div>
      </div>

      {/* 汇总卡片 */}
      <UsageSummaryCards data={summary.data} loading={summary.loading} />

      {/* 趋势图 */}
      <UsageTrendChart data={trends.data} loading={trends.loading} timeRange={timeRange} />

      {/* 请求日志 / 模型统计 */}
      <Tabs defaultValue="logs">
        <TabsList>
          <TabsTrigger value="logs">请求日志</TabsTrigger>
          <TabsTrigger value="models">模型统计</TabsTrigger>
          <TabsTrigger value="keys" className="gap-1.5">
            <KeyRound className="h-4 w-4" />
            密钥管理
          </TabsTrigger>
        </TabsList>
        <TabsContent value="logs">
          <RequestLogTable timeRange={timeRange} refreshMs={refreshMs} />
        </TabsContent>
        <TabsContent value="models">
          <ModelStatsTable data={models.data} loading={models.loading} />
        </TabsContent>
        <TabsContent value="keys">
          <ApiKeyManager />
        </TabsContent>
      </Tabs>
    </div>
  );
}
