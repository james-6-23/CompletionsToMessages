import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import type { ModelStats } from '@/types/usage';
import { fmtInt, fmtUsd } from './format';

interface Props {
  data: ModelStats[];
  loading: boolean;
}

export function ModelStatsTable({ data, loading }: Props) {
  if (loading) {
    return (
      <div className="flex h-40 items-center justify-center text-muted-foreground">
        <div className="flex items-center gap-2">
          <div className="spinner !w-5 !h-5 !border-2" />
          加载中...
        </div>
      </div>
    );
  }

  if (!data.length) {
    return (
      <div className="flex h-40 items-center justify-center text-muted-foreground">暂无数据</div>
    );
  }

  return (
    <div className="rounded-2xl border border-border/50 bg-card shadow-sm overflow-hidden">
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>模型</TableHead>
            <TableHead className="text-right">请求数</TableHead>
            <TableHead className="text-right">总 Tokens</TableHead>
            <TableHead className="text-right">总成本</TableHead>
            <TableHead className="text-right">平均成本</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {data.map((row) => (
            <TableRow key={row.model}>
              <TableCell className="font-mono text-sm">{row.model}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.request_count)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.total_tokens)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtUsd(row.total_cost, 4)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtUsd(row.avg_cost_per_request, 4)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
