import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { api } from '@/lib/api';
import { toast } from '@/components/Toast';
import type { PplxPoolStatus, PplxClientStatus } from '@/types/usage';
import {
  Plus, Trash2, Loader2, RefreshCw, CheckCircle2,
  XCircle, AlertCircle, Clock, Wifi, WifiOff, Eye, EyeOff,
} from 'lucide-react';

// ==================== 状态徽标 ====================

function StatusBadge({ client }: { client: PplxClientStatus }) {
  if (!client.enabled) {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs font-medium text-muted-foreground">
        <XCircle className="size-3" /> 已禁用
      </span>
    );
  }
  if (client.state === 'offline') {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-red-500/10 px-2 py-0.5 text-xs font-medium text-red-500">
        <XCircle className="size-3" /> 离线
      </span>
    );
  }
  if (!client.available) {
    return (
      <span className="inline-flex items-center gap-1 rounded-full bg-yellow-500/10 px-2 py-0.5 text-xs font-medium text-yellow-500">
        <Clock className="size-3" /> 冷却中
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1 rounded-full bg-emerald-500/10 px-2 py-0.5 text-xs font-medium text-emerald-500">
      <CheckCircle2 className="size-3" /> 可用
    </span>
  );
}

// ==================== 单条账号行 ====================

function AccountRow({
  client,
  onAction,
}: {
  client: PplxClientStatus;
  onAction: (action: string, id: string) => Promise<void>;
}) {
  const [loading, setLoading] = useState<string | null>(null);

  const handle = async (action: string) => {
    setLoading(action);
    try {
      await onAction(action, client.id);
    } finally {
      setLoading(null);
    }
  };

  return (
    <div className="flex items-center gap-3 rounded-xl border border-border/60 bg-card px-4 py-3 transition-colors hover:bg-muted/30">
      {/* 账号 ID */}
      <div className="flex-1 min-w-0">
        <p className="text-sm font-medium truncate">{client.id}</p>
        <p className="text-xs text-muted-foreground mt-0.5">
          请求 {client.request_count} 次 · 权重 {client.weight}
          {client.next_available_at && (
            <> · 恢复于 {new Date(client.next_available_at).toLocaleTimeString()}</>
          )}
        </p>
      </div>

      {/* 状态 */}
      <StatusBadge client={client} />

      {/* 操作按钮 */}
      <div className="flex items-center gap-1.5 shrink-0">
        {/* 启用 / 禁用 */}
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-2.5 text-xs"
          disabled={loading !== null}
          onClick={() => handle(client.enabled ? 'disable' : 'enable')}
          title={client.enabled ? '禁用' : '启用'}
        >
          {loading === 'enable' || loading === 'disable' ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : client.enabled ? (
            <EyeOff className="size-3.5" />
          ) : (
            <Eye className="size-3.5" />
          )}
        </Button>

        {/* 重置失败计数 */}
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-2.5 text-xs"
          disabled={loading !== null}
          onClick={() => handle('reset')}
          title="重置失败计数"
        >
          {loading === 'reset' ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <RefreshCw className="size-3.5" />
          )}
        </Button>

        {/* 删除 */}
        <Button
          variant="ghost"
          size="sm"
          className="h-8 px-2.5 text-xs text-destructive hover:text-destructive hover:bg-destructive/10"
          disabled={loading !== null}
          onClick={() => handle('remove')}
          title="删除账号"
        >
          {loading === 'remove' ? (
            <Loader2 className="size-3.5 animate-spin" />
          ) : (
            <Trash2 className="size-3.5" />
          )}
        </Button>
      </div>
    </div>
  );
}

// ==================== 添加账号表单 ====================

function AddAccountForm({ onAdd }: { onAdd: () => void }) {
  const [id, setId] = useState('');
  const [csrf, setCsrf] = useState('');
  const [session, setSession] = useState('');
  const [loading, setLoading] = useState(false);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!id.trim() || !csrf.trim() || !session.trim()) {
      toast('请填写所有字段', 'error');
      return;
    }
    setLoading(true);
    try {
      const res = await api.pplxPoolAction('add', {
        id: id.trim(),
        csrf_token: csrf.trim(),
        session_token: session.trim(),
      });
      if (res.status === 'ok') {
        toast('账号添加成功', 'success');
        setId(''); setCsrf(''); setSession('');
        onAdd();
      } else {
        toast(res.message ?? '添加失败', 'error');
      }
    } catch {
      toast('添加失败，请检查服务连接', 'error');
    } finally {
      setLoading(false);
    }
  };

  return (
    <form onSubmit={handleSubmit} className="rounded-xl border border-dashed border-border/60 bg-muted/20 p-4 space-y-3">
      <p className="text-sm font-medium text-foreground">添加 Perplexity 账号</p>

      <Input
        placeholder="账号 ID（如邮箱）"
        value={id}
        onChange={e => setId(e.target.value)}
        className="h-9 text-sm"
      />

      <Input
        placeholder="CSRF Token（next-auth.csrf-token）"
        value={csrf}
        onChange={e => setCsrf(e.target.value)}
        className="h-9 text-sm font-mono text-xs"
      />

      <textarea
        placeholder="Session Token（__Secure-next-auth.session-token）"
        value={session}
        onChange={e => setSession(e.target.value)}
        rows={3}
        className="w-full resize-none rounded-md border border-input bg-background px-3 py-2 text-xs font-mono text-foreground placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-ring focus:ring-offset-2 disabled:opacity-50"
      />

      <p className="text-[11px] text-muted-foreground">
        打开 perplexity.ai → F12 → Application → Cookies 获取上述两个 Token
      </p>

      <Button type="submit" size="sm" className="w-full gap-2" disabled={loading}>
        {loading ? <Loader2 className="size-3.5 animate-spin" /> : <Plus className="size-3.5" />}
        添加账号
      </Button>
    </form>
  );
}

// ==================== 主组件 ====================

export function PerplexityManager() {
  const [status, setStatus] = useState<PplxPoolStatus | null>(null);
  const [loading, setLoading] = useState(true);
  const [connected, setConnected] = useState(false);
  const [showAdd, setShowAdd] = useState(false);

  const refresh = useCallback(async () => {
    try {
      const data = await api.pplxStatus();
      setStatus(data);
      setConnected(true);
    } catch {
      setConnected(false);
      setStatus(null);
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleAction = async (action: string, id: string) => {
    try {
      const res = await api.pplxPoolAction(action, { id });
      if (res.status === 'ok') {
        const msgs: Record<string, string> = {
          enable: '已启用', disable: '已禁用', remove: '已删除', reset: '已重置',
        };
        toast(msgs[action] ?? '操作成功', 'success');
        await refresh();
      } else {
        toast(res.message ?? '操作失败', 'error');
      }
    } catch {
      toast('操作失败，请检查服务连接', 'error');
    }
  };

  return (
    <div className="space-y-6">
      {/* 页头 */}
      <div className="flex items-center justify-between">
        <div>
          <h2 className="text-lg font-bold">Perplexity 搜索</h2>
          <p className="text-sm text-muted-foreground mt-0.5">管理 Perplexity 账号池，为模型提供联网搜索能力</p>
        </div>
        <div className="flex items-center gap-2">
          <Button variant="outline" size="sm" className="gap-2" onClick={refresh} disabled={loading}>
            <RefreshCw className={`size-3.5 ${loading ? 'animate-spin' : ''}`} />
            刷新
          </Button>
          <Button size="sm" className="gap-2" onClick={() => setShowAdd(v => !v)}>
            <Plus className="size-3.5" />
            添加账号
          </Button>
        </div>
      </div>

      {/* 连接状态 + 统计卡片 */}
      <div className="grid grid-cols-3 gap-4">
        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground mb-2">服务状态</p>
          <div className="flex items-center gap-2">
            {connected ? (
              <>
                <Wifi className="size-4 text-emerald-500" />
                <span className="text-sm font-semibold text-emerald-500">已连接</span>
              </>
            ) : (
              <>
                <WifiOff className="size-4 text-destructive" />
                <span className="text-sm font-semibold text-destructive">未连接</span>
              </>
            )}
          </div>
        </div>

        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground mb-2">账号总数</p>
          <p className="text-2xl font-bold">{status?.total ?? '—'}</p>
        </div>

        <div className="rounded-xl border border-border bg-card p-4">
          <p className="text-xs text-muted-foreground mb-2">可用账号</p>
          <p className="text-2xl font-bold text-emerald-500">{status?.available ?? '—'}</p>
        </div>
      </div>

      {/* 添加账号表单 */}
      {showAdd && (
        <AddAccountForm
          onAdd={() => {
            setShowAdd(false);
            refresh();
          }}
        />
      )}

      {/* 未连接提示 */}
      {!loading && !connected && (
        <div className="rounded-xl border border-border bg-muted/30 p-8 text-center">
          <AlertCircle className="size-8 text-muted-foreground mx-auto mb-3" />
          <p className="text-sm font-medium">无法连接到 Perplexity 服务</p>
          <p className="text-xs text-muted-foreground mt-1">
            请确认 docker compose 已启动，且 PPLX_SERVICE_URL 配置正确
          </p>
        </div>
      )}

      {/* 账号列表 */}
      {connected && (
        <div className="space-y-2">
          {loading ? (
            <div className="flex items-center justify-center py-12">
              <Loader2 className="size-5 animate-spin text-muted-foreground" />
            </div>
          ) : status?.clients && status.clients.length > 0 ? (
            status.clients.map(client => (
              <AccountRow key={client.id} client={client} onAction={handleAction} />
            ))
          ) : (
            <div className="rounded-xl border border-dashed border-border bg-muted/20 p-8 text-center">
              <p className="text-sm text-muted-foreground">暂无账号，点击「添加账号」开始配置</p>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
