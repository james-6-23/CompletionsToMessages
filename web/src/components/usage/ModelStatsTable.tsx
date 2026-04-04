import { Table, TableHeader, TableBody, TableRow, TableHead, TableCell } from '@/components/ui/table';
import type { ModelStats } from '@/types/usage';
import { fmtInt, fmtDuration } from './format';

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
            <TableHead className="text-right">Input</TableHead>
            <TableHead className="text-right">Output</TableHead>
            <TableHead className="text-right">缓存读取</TableHead>
            <TableHead className="text-right">缓存创建</TableHead>
            <TableHead className="text-right">平均耗时</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {data.map((row) => (
            <TableRow key={row.model}>
              <TableCell className="font-mono text-sm">{row.model}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.request_count)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.input_tokens)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.output_tokens)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.cache_read_tokens)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtInt(row.cache_creation_tokens)}</TableCell>
              <TableCell className="text-right tabular-nums">{fmtDuration(Math.round(row.avg_latency_ms))}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
    </div>
  );
}
