import { Activity, DollarSign, Layers, Database, Gauge } from 'lucide-react';
import type { UsageSummary } from '@/types/usage';
import { fmtInt, fmtUsd, parseFiniteNumber } from './format';

interface Props {
  data: UsageSummary | null;
  loading: boolean;
  rpm: number | null;
}

interface StatCardProps {
  icon: React.ReactNode;
  label: string;
  value: string;
  subValues?: { label: string; value: string }[];
  iconBg: string;
  iconColor: string;
}

function StatCard({ icon, label, value, subValues, iconBg, iconColor }: StatCardProps) {
  return (
    <div className="rounded-2xl border border-border/50 bg-card p-5 shadow-sm transition-all duration-200 hover:shadow-md hover:-translate-y-0.5">
      <div className="flex items-center gap-4">
        <div className={`flex h-12 w-12 items-center justify-center rounded-xl ${iconBg}`}>
          <span className={iconColor}>{icon}</span>
        </div>
        <div className="flex-1 min-w-0">
          <p className="text-xs font-semibold uppercase tracking-wider text-muted-foreground">{label}</p>
          <p className="text-2xl font-bold tracking-tight mt-0.5">{value}</p>
          {subValues && (
            <div className="mt-1.5 flex gap-3 text-xs text-muted-foreground">
              {subValues.map((sv) => (
                <span key={sv.label}>
                  {sv.label}: <span className="font-medium text-foreground">{sv.value}</span>
                </span>
              ))}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

export function UsageSummaryCards({ data, loading, rpm }: Props) {
  if (loading || !data) {
    return (
      <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
        {Array.from({ length: 5 }).map((_, i) => (
          <div key={i} className="h-[108px] animate-pulse rounded-2xl border border-border/50 bg-card" />
        ))}
      </div>
    );
  }

  const totalTokens = data.total_input_tokens + data.total_output_tokens;
  const costNum = parseFiniteNumber(data.total_cost);
  const costDigits = costNum !== null && costNum < 1 ? 4 : 2;

  return (
    <div className="grid grid-cols-1 gap-4 sm:grid-cols-2 lg:grid-cols-5">
      <StatCard
        icon={<Gauge className="h-6 w-6" />}
        label="实时 RPM"
        value={rpm !== null ? fmtInt(rpm) : '-'}
        iconBg="bg-rose-500/10"
        iconColor="text-rose-500"
      />
      <StatCard
        icon={<Activity className="h-6 w-6" />}
        label="总请求数"
        value={fmtInt(data.total_requests)}
        iconBg="bg-blue-500/10"
        iconColor="text-blue-500"
      />
      <StatCard
        icon={<DollarSign className="h-6 w-6" />}
        label="总成本"
        value={fmtUsd(data.total_cost, costDigits)}
        iconBg="bg-emerald-500/10"
        iconColor="text-emerald-500"
      />
      <StatCard
        icon={<Layers className="h-6 w-6" />}
        label="总 Token 数"
        value={fmtInt(totalTokens)}
        subValues={[
          { label: '输入', value: fmtInt(data.total_input_tokens) },
          { label: '输出', value: fmtInt(data.total_output_tokens) },
        ]}
        iconBg="bg-violet-500/10"
        iconColor="text-violet-500"
      />
      <StatCard
        icon={<Database className="h-6 w-6" />}
        label="缓存 Token"
        value={fmtInt(data.total_cache_creation_tokens + data.total_cache_read_tokens)}
        subValues={[
          { label: '创建', value: fmtInt(data.total_cache_creation_tokens) },
          { label: '命中', value: fmtInt(data.total_cache_read_tokens) },
        ]}
        iconBg="bg-amber-500/10"
        iconColor="text-amber-500"
      />
    </div>
  );
}
