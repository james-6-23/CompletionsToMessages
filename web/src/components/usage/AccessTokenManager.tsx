import { useState, useEffect, useCallback } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { api } from '@/lib/api';
import { copyToClipboard } from '@/lib/utils';
import type { AccessToken, Endpoint } from '@/types/usage';
import {
  Plus, Trash2, Copy, KeyRound, Loader2, Check, Power,
} from 'lucide-react';
import { fmtTimestamp } from './format';

export function AccessTokenManager() {
  const [tokens, setTokens] = useState<AccessToken[]>([]);
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [loading, setLoading] = useState(true);
  const [showAdd, setShowAdd] = useState(false);
  const [newName, setNewName] = useState('');
  const [selectedChannels, setSelectedChannels] = useState<Set<string>>(new Set());
  const [createdToken, setCreatedToken] = useState<string | null>(null);

  const refresh = useCallback(() => {
    Promise.all([
      api.listAccessTokens(),
      api.listEndpoints(),
    ]).then(([t, e]) => {
      setTokens(t);
      setEndpoints(e);
    }).catch(console.error).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
    const timer = setInterval(refresh, 30000);
    return () => clearInterval(timer);
  }, [refresh]);

  async function handleCreate() {
    try {
      const result = await api.addAccessToken({
        name: newName.trim() || '默认密钥',
        channel_ids: Array.from(selectedChannels),
      });
      setCreatedToken(result.token);
      setNewName('');
      setSelectedChannels(new Set());
      setShowAdd(false);
      refresh();
    } catch (e) {
      console.error('创建访问密钥失败:', e);
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此访问密钥？使用该密钥的客户端将无法访问。')) return;
    try {
      await api.deleteAccessToken(id);
      refresh();
    } catch (e) {
      console.error('删除访问密钥失败:', e);
    }
  }

  async function handleToggle(id: string, currentActive: boolean) {
    try {
      await api.toggleAccessToken(id, !currentActive);
      refresh();
    } catch (e) {
      console.error('切换状态失败:', e);
    }
  }

  function toggleChannel(id: string) {
    setSelectedChannels(prev => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  }

  function handleCopy(text: string) {
    copyToClipboard(text);
  }

  function getChannelName(id: string): string {
    return endpoints.find(e => e.id === id)?.name || id.slice(0, 8);
  }

  return (
    <div className="space-y-6">
      {/* 页头 */}
      <div>
        <h2 className="text-[clamp(28px,4vw,38px)] font-semibold leading-[1.08] tracking-tight">
          访问密钥
        </h2>
        <p className="mt-2 text-muted-foreground text-[15px] leading-relaxed max-w-[600px]">
          管理下游客户端的访问密钥。客户端设置 <code className="bg-muted px-1.5 py-0.5 rounded text-xs font-mono">ANTHROPIC_API_KEY</code> 为此密钥即可访问。
        </p>
      </div>

      {/* 创建后显示完整 token */}
      {createdToken && (
        <Card className="border-emerald-500/30 bg-emerald-500/[0.03]">
          <CardContent className="p-5 space-y-2">
            <p className="text-sm font-semibold text-emerald-600 dark:text-emerald-400">密钥创建成功</p>
            <div className="flex items-center gap-2">
              <code className="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 px-3 py-2 rounded-lg font-mono text-sm flex-1 break-all select-all">
                {createdToken}
              </code>
              <Button variant="ghost" size="icon" className="h-9 w-9 shrink-0" onClick={() => handleCopy(createdToken)}>
                <Copy className="h-4 w-4" />
              </Button>
            </div>
            <p className="text-xs text-amber-500">请立即复制，关闭后将不再显示完整密钥。</p>
            <Button size="sm" variant="outline" onClick={() => setCreatedToken(null)}>关闭</Button>
          </CardContent>
        </Card>
      )}

      {/* 工具栏 */}
      <Button size="sm" onClick={() => setShowAdd(!showAdd)}>
        <Plus className="h-4 w-4" /> 创建访问密钥
      </Button>

      {/* 创建表单 */}
      {showAdd && (
        <Card className="border-border/50">
          <CardContent className="p-5 space-y-4">
            <p className="text-sm font-semibold">创建访问密钥</p>
            <Input
              placeholder="密钥名称 (可选，如: Claude Code 主力)"
              value={newName}
              onChange={e => setNewName(e.target.value)}
            />

            <div>
              <p className="text-xs font-semibold text-muted-foreground mb-2">绑定渠道（选择允许访问的渠道）</p>
              {endpoints.length === 0 ? (
                <p className="text-xs text-muted-foreground">暂无渠道，请先在「渠道管理」中添加</p>
              ) : (
                <div className="flex flex-wrap gap-2">
                  {endpoints.map(ep => (
                    <button
                      key={ep.id}
                      onClick={() => toggleChannel(ep.id)}
                      className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors ${
                        selectedChannels.has(ep.id)
                          ? 'border-primary bg-primary/10 text-primary'
                          : 'border-border text-muted-foreground hover:border-primary/50'
                      }`}
                    >
                      {selectedChannels.has(ep.id) && <Check className="h-3 w-3" />}
                      {ep.name}
                      {!ep.is_active && <span className="text-red-400">(停用)</span>}
                    </button>
                  ))}
                </div>
              )}
            </div>

            <div className="flex gap-2">
              <Button size="sm" onClick={handleCreate} disabled={endpoints.length === 0}>
                确认创建
              </Button>
              <Button size="sm" variant="outline" onClick={() => { setShowAdd(false); setSelectedChannels(new Set()); }}>
                取消
              </Button>
            </div>
          </CardContent>
        </Card>
      )}

      {/* 加载 */}
      {loading && tokens.length === 0 && (
        <div className="flex items-center justify-center py-12 text-muted-foreground">
          <Loader2 className="h-5 w-5 animate-spin mr-3" /> 加载中...
        </div>
      )}

      {/* 空状态 */}
      {!loading && tokens.length === 0 && !createdToken && (
        <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
          <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted mb-4">
            <KeyRound className="h-8 w-8 opacity-40" />
          </div>
          <p className="text-lg font-semibold text-foreground">暂无访问密钥</p>
          <p className="text-sm mt-1">创建访问密钥后，客户端即可通过该密钥请求上游 API</p>
        </div>
      )}

      {/* 密钥列表 */}
      <div className="space-y-3">
        {tokens.map(token => (
          <AccessTokenCard
            key={token.id}
            token={token}
            endpoints={endpoints}
            getChannelName={getChannelName}
            onDelete={() => handleDelete(token.id)}
            onToggle={() => handleToggle(token.id, token.is_active)}
            onRefresh={refresh}
          />
        ))}
      </div>
    </div>
  );
}

function AccessTokenCard({
  token,
  endpoints,
  getChannelName,
  onDelete,
  onToggle,
  onRefresh,
}: {
  token: AccessToken;
  endpoints: Endpoint[];
  getChannelName: (id: string) => string;
  onDelete: () => void;
  onToggle: () => void;
  onRefresh: () => void;
}) {
  const [editingChannels, setEditingChannels] = useState(false);
  const [selected, setSelected] = useState<Set<string>>(new Set(token.channel_ids));
  const [saving, setSaving] = useState(false);

  function toggleChannel(id: string) {
    setSelected(prev => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  }

  async function handleSaveChannels() {
    setSaving(true);
    try {
      await api.updateAccessTokenChannels(token.id, Array.from(selected));
      setEditingChannels(false);
      onRefresh();
    } catch (e) {
      console.error('更新渠道绑定失败:', e);
    } finally {
      setSaving(false);
    }
  }

  function handleCopy(text: string) {
    copyToClipboard(text);
  }

  return (
    <Card className={`rounded-2xl transition-all duration-200 hover:shadow-md ${token.is_active ? 'border-border/50' : 'border-red-500/20 bg-red-500/[0.02]'}`}>
      <CardContent className="p-5 space-y-3">
        {/* 头部 */}
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-3 min-w-0">
            <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-xl ${token.is_active ? 'bg-primary/10' : 'bg-red-500/10'}`}>
              <KeyRound className={`h-4.5 w-4.5 ${token.is_active ? 'text-primary' : 'text-red-400'}`} />
            </div>
            <div className="min-w-0">
              <div className="flex items-center gap-2">
                <span className="text-sm font-semibold truncate">{token.name || '未命名'}</span>
                <Badge
                  variant={token.is_active ? 'default' : 'destructive'}
                  className={`text-xs shrink-0 ${token.is_active ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20' : ''}`}
                >
                  {token.is_active ? '有效' : '停用'}
                </Badge>
              </div>
              <div className="flex items-center gap-1.5 mt-0.5">
                <code className="text-xs font-mono text-muted-foreground bg-muted px-1.5 py-0.5 rounded">{token.token_masked}</code>
              </div>
            </div>
          </div>

          {/* 统计 + 操作 */}
          <div className="flex items-center gap-4 shrink-0">
            <div className="hidden sm:flex items-center gap-4 text-xs text-muted-foreground">
              <span>请求 <strong className="text-foreground">{token.total_requests.toLocaleString()}</strong></span>
              <span>失败 <strong className={token.failed_requests > 0 ? 'text-red-500' : 'text-foreground'}>{token.failed_requests}</strong></span>
              {token.last_used_at && <span>最近 {fmtTimestamp(token.last_used_at)}</span>}
            </div>
            <div className="flex items-center gap-1">
              <Button variant="ghost" size="icon" className="h-8 w-8" title={token.is_active ? '停用' : '启用'} onClick={onToggle}>
                <Power className={`h-3.5 w-3.5 ${token.is_active ? 'text-emerald-500' : 'text-red-400'}`} />
              </Button>
              <Button variant="ghost" size="icon" className="h-8 w-8 text-red-500 hover:text-red-600" title="删除" onClick={onDelete}>
                <Trash2 className="h-3.5 w-3.5" />
              </Button>
            </div>
          </div>
        </div>

        {/* 绑定渠道 */}
        <div className="flex items-start gap-2">
          <span className="text-xs text-muted-foreground shrink-0 pt-1">绑定渠道:</span>
          <div className="flex flex-wrap gap-1.5">
            {token.channel_ids.length === 0 ? (
              <span className="text-xs text-amber-500">未绑定任何渠道</span>
            ) : (
              token.channel_ids.map(cid => (
                <Badge key={cid} variant="outline" className="text-xs">
                  {getChannelName(cid)}
                </Badge>
              ))
            )}
            <button
              onClick={() => { setEditingChannels(!editingChannels); setSelected(new Set(token.channel_ids)); }}
              className="text-xs text-primary hover:underline"
            >
              {editingChannels ? '取消' : '编辑'}
            </button>
          </div>
        </div>

        {/* 编辑渠道绑定 */}
        {editingChannels && (
          <div className="space-y-3 pt-2 border-t border-border/50">
            <div className="flex flex-wrap gap-2">
              {endpoints.map(ep => (
                <button
                  key={ep.id}
                  onClick={() => toggleChannel(ep.id)}
                  className={`inline-flex items-center gap-1.5 px-3 py-1.5 rounded-lg border text-xs font-medium transition-colors ${
                    selected.has(ep.id)
                      ? 'border-primary bg-primary/10 text-primary'
                      : 'border-border text-muted-foreground hover:border-primary/50'
                  }`}
                >
                  {selected.has(ep.id) && <Check className="h-3 w-3" />}
                  {ep.name}
                </button>
              ))}
            </div>
            <div className="flex gap-2">
              <Button size="sm" onClick={handleSaveChannels} disabled={saving}>
                {saving ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : null}
                保存
              </Button>
              <Button size="sm" variant="outline" onClick={() => setEditingChannels(false)}>取消</Button>
            </div>
          </div>
        )}
      </CardContent>
    </Card>
  );
}
