import { useState, useEffect, useCallback } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { api } from '@/lib/api';
import { copyToClipboard } from '@/lib/utils';
import type { ApiKey, Endpoint } from '@/types/usage';
import {
  Plus, Trash2, Eye, EyeOff, Copy, FlaskConical, Check, X,
  Loader2, KeyRound, Save, ChevronDown,
  ChevronRight, Globe, Pencil, Power,
} from 'lucide-react';
import { fmtTimestamp } from './format';

/* ------------------------------------------------------------------ */
/*  单个 Key 卡片                                                      */
/* ------------------------------------------------------------------ */

function KeyCard({ apiKey, isTesting, testResult, isRevealed, onToggle, onDelete, onTest, onCopy, onToggleReveal }: {
  apiKey: ApiKey;
  isTesting: boolean;
  testResult: boolean | null | undefined;
  isRevealed: boolean;
  onToggle: () => void;
  onDelete: () => void;
  onTest: () => void;
  onCopy: () => void;
  onToggleReveal: () => void;
}) {
  return (
    <Card className={`transition-all duration-200 hover:shadow-md hover:-translate-y-0.5 ${apiKey.is_active ? 'border-emerald-500/20 bg-emerald-500/[0.03]' : 'border-red-500/20 bg-red-500/[0.03]'}`}>
      <CardContent className="p-5 space-y-3">
        {/* 头部 */}
        <div className="flex items-center justify-between">
          <Badge
            variant={apiKey.is_active ? 'default' : 'destructive'}
            className={`text-xs cursor-pointer transition-colors ${apiKey.is_active ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 hover:bg-emerald-500/20 border-emerald-500/20' : ''}`}
            onClick={onToggle}
          >
            {apiKey.is_active ? '有效' : '失效'}
          </Badge>
          <code className="text-sm font-mono bg-muted px-2.5 py-1 rounded-lg">
            {apiKey.api_key_masked}
          </code>
        </div>

        {/* 标签 */}
        {apiKey.label && (
          <p className="text-sm text-muted-foreground truncate">{apiKey.label}</p>
        )}

        {/* 操作按钮行 */}
        <div className="flex items-center gap-1">
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onToggleReveal} title="显示/隐藏">
            {isRevealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </Button>
          <Button variant="ghost" size="icon" className="h-8 w-8" onClick={onCopy} title="复制">
            <Copy className="h-3.5 w-3.5" />
          </Button>
        </div>

        {/* 统计数据 */}
        <div className="flex items-center gap-4 text-xs text-muted-foreground">
          <span>请求 <strong className="text-foreground">{apiKey.total_requests}</strong></span>
          <span>失败 <strong className={apiKey.failed_requests > 0 ? 'text-red-500' : 'text-foreground'}>{apiKey.failed_requests}</strong></span>
          {apiKey.last_used_at && (
            <span className="truncate">最近 {fmtTimestamp(apiKey.last_used_at)}</span>
          )}
        </div>

        {/* 操作行 */}
        <div className="flex items-center justify-between pt-2 border-t border-border/50">
          <Button
            variant="ghost"
            size="sm"
            className="text-blue-500 hover:text-blue-600 h-8 text-xs"
            onClick={onTest}
            disabled={isTesting}
          >
            {isTesting ? <Loader2 className="h-3 w-3 animate-spin" /> : <FlaskConical className="h-3 w-3" />}
            测试
            {testResult === true && <Check className="h-3 w-3 text-emerald-500" />}
            {testResult === false && <X className="h-3 w-3 text-red-500" />}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="text-red-500 hover:text-red-600 h-8 text-xs"
            onClick={onDelete}
          >
            <Trash2 className="h-3 w-3" /> 删除
          </Button>
        </div>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  端点卡片（可折叠，包含密钥列表）                                     */
/* ------------------------------------------------------------------ */

function EndpointCard({
  endpoint,
  keys,
  expanded,
  onToggleExpand,
  onRefresh,
}: {
  endpoint: Endpoint;
  keys: ApiKey[];
  expanded: boolean;
  onToggleExpand: () => void;
  onRefresh: () => void;
}) {
  /* 编辑模式 */
  const [editing, setEditing] = useState(false);
  const [editName, setEditName] = useState(endpoint.name);
  const [editUrl, setEditUrl] = useState(endpoint.base_url);
  const [saving, setSaving] = useState(false);

  /* 新增密钥弹窗 */
  const [showAddKey, setShowAddKey] = useState(false);
  const [newKeysText, setNewKeysText] = useState('');
  const [adding, setAdding] = useState(false);

  /* Key 交互状态 */
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());
  const [testingKeys, setTestingKeys] = useState<Set<string>>(new Set());
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});

  /* 保存端点编辑 */
  async function handleSaveEndpoint() {
    if (!editName.trim() || !editUrl.trim()) return;
    setSaving(true);
    try {
      await api.updateEndpoint(endpoint.id, { name: editName.trim(), base_url: editUrl.trim() });
      setEditing(false);
      onRefresh();
    } catch (e) {
      console.error('保存端点失败:', e);
    } finally {
      setSaving(false);
    }
  }

  /* 删除端点 */
  async function handleDeleteEndpoint() {
    if (!confirm(`确定删除端点「${endpoint.name}」？关联的所有密钥也会被删除。`)) return;
    try {
      await api.deleteEndpoint(endpoint.id);
      onRefresh();
    } catch (e) {
      console.error('删除端点失败:', e);
    }
  }

  /* 切换端点状态 */
  async function handleToggleEndpoint() {
    try {
      await api.toggleEndpoint(endpoint.id, !endpoint.is_active);
      onRefresh();
    } catch (e) {
      console.error('切换端点状态失败:', e);
    }
  }

  /* 批量添加密钥 */
  async function handleAddKeys() {
    const lines = newKeysText.split('\n').map(l => l.trim()).filter(l => l.length > 0);
    if (lines.length === 0) return;
    setAdding(true);
    try {
      for (const line of lines) {
        await api.addApiKey({ endpoint_id: endpoint.id, api_key: line, label: '' });
      }
      setNewKeysText('');
      setShowAddKey(false);
      onRefresh();
    } catch (e) {
      console.error('添加密钥失败:', e);
    } finally {
      setAdding(false);
    }
  }

  /* 文件上传密钥 */
  function handleFileUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const text = ev.target?.result as string;
      if (text) {
        setNewKeysText(prev => prev ? prev + '\n' + text.trim() : text.trim());
      }
    };
    reader.readAsText(file);
    e.target.value = '';
  }

  /* 密钥操作 */
  async function handleDeleteKey(id: string) {
    if (!confirm('确定删除此密钥？')) return;
    try {
      await api.deleteApiKey(id);
      onRefresh();
    } catch (e) {
      console.error('删除密钥失败:', e);
    }
  }

  async function handleToggleKey(id: string, currentActive: boolean) {
    try {
      await api.toggleApiKey(id, !currentActive);
      onRefresh();
    } catch (e) {
      console.error('切换密钥状态失败:', e);
    }
  }

  async function handleTestKey(id: string) {
    setTestingKeys(prev => new Set(prev).add(id));
    setTestResults(prev => ({ ...prev, [id]: null }));
    try {
      const result = await api.testApiKey(id);
      setTestResults(prev => ({ ...prev, [id]: result.valid }));
    } catch {
      setTestResults(prev => ({ ...prev, [id]: false }));
    } finally {
      setTestingKeys(prev => { const s = new Set(prev); s.delete(id); return s; });
    }
  }

  async function handleCopyKey(id: string) {
    try {
      const { api_key } = await api.getApiKeyFull(id);
      copyToClipboard(api_key);
    } catch (e) {
      console.error('获取完整密钥失败:', e);
    }
  }

  function toggleRevealKey(id: string) {
    setRevealedKeys(prev => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  }

  const activeKeyCount = keys.filter(k => k.is_active).length;
  const totalRequests = keys.reduce((sum, k) => sum + k.total_requests, 0);

  return (
    <Card className={`rounded-2xl transition-all duration-200 hover:shadow-md ${endpoint.is_active ? 'border-border/50' : 'border-red-500/20 bg-red-500/[0.02]'}`}>
      <CardContent className="p-0">
        {/* 端点头部 - 可点击折叠 */}
        <div
          className="flex items-center gap-3 p-5 cursor-pointer select-none"
          onClick={onToggleExpand}
        >
          {/* 折叠图标 */}
          <div className="shrink-0 text-muted-foreground">
            {expanded
              ? <ChevronDown className="h-4 w-4" />
              : <ChevronRight className="h-4 w-4" />}
          </div>

          {/* 端点图标 */}
          <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-xl ${endpoint.is_active ? 'bg-blue-500/10' : 'bg-red-500/10'}`}>
            <Globe className={`h-4.5 w-4.5 ${endpoint.is_active ? 'text-blue-500' : 'text-red-400'}`} />
          </div>

          {/* 端点信息 */}
          <div className="flex-1 min-w-0">
            <div className="flex items-center gap-2">
              <span className="text-sm font-semibold truncate">{endpoint.name}</span>
              <Badge
                variant={endpoint.is_active ? 'default' : 'destructive'}
                className={`text-xs shrink-0 ${endpoint.is_active ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 border-emerald-500/20' : ''}`}
              >
                {endpoint.is_active ? '活跃' : '停用'}
              </Badge>
            </div>
            <p className="text-xs text-muted-foreground truncate mt-0.5">{endpoint.base_url}</p>
          </div>

          {/* 右侧统计 */}
          <div className="flex items-center gap-4 text-xs text-muted-foreground shrink-0">
            <span>密钥 <strong className="text-foreground">{keys.length}</strong> (<span className="text-emerald-500">{activeKeyCount}</span>)</span>
            <span>请求 <strong className="text-foreground">{totalRequests.toLocaleString()}</strong></span>
          </div>

          {/* 操作按钮（阻止冒泡） */}
          <div className="flex items-center gap-1 shrink-0" onClick={e => e.stopPropagation()}>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              title={endpoint.is_active ? '停用端点' : '启用端点'}
              onClick={handleToggleEndpoint}
            >
              <Power className={`h-3.5 w-3.5 ${endpoint.is_active ? 'text-emerald-500' : 'text-red-400'}`} />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8"
              title="编辑端点"
              onClick={() => {
                setEditName(endpoint.name);
                setEditUrl(endpoint.base_url);
                setEditing(true);
                if (!expanded) onToggleExpand();
              }}
            >
              <Pencil className="h-3.5 w-3.5" />
            </Button>
            <Button
              variant="ghost"
              size="icon"
              className="h-8 w-8 text-red-500 hover:text-red-600"
              title="删除端点"
              onClick={handleDeleteEndpoint}
            >
              <Trash2 className="h-3.5 w-3.5" />
            </Button>
          </div>
        </div>

        {/* 展开区域 */}
        {expanded && (
          <div className="border-t border-border/50 p-5 pt-4 space-y-4">
            {/* 编辑模式 */}
            {editing && (
              <Card className="border-border/50 bg-muted/30">
                <CardContent className="p-4 space-y-3">
                  <p className="text-xs font-semibold text-muted-foreground">编辑端点</p>
                  <div className="flex flex-col gap-3 sm:flex-row">
                    <Input
                      placeholder="端点名称"
                      value={editName}
                      onChange={e => setEditName(e.target.value)}
                      className="sm:w-48"
                    />
                    <Input
                      placeholder="https://api.example.com"
                      value={editUrl}
                      onChange={e => setEditUrl(e.target.value)}
                      className="flex-1"
                    />
                    <div className="flex gap-2">
                      <Button size="sm" onClick={handleSaveEndpoint} disabled={saving || !editName.trim() || !editUrl.trim()}>
                        {saving ? <Loader2 className="h-4 w-4 animate-spin" /> : <Save className="h-4 w-4" />}
                        保存
                      </Button>
                      <Button size="sm" variant="outline" onClick={() => setEditing(false)}>取消</Button>
                    </div>
                  </div>
                </CardContent>
              </Card>
            )}

            {/* 添加密钥按钮 */}
            <div className="flex items-center gap-3">
              <Button size="sm" onClick={() => setShowAddKey(true)}>
                <Plus className="h-4 w-4" /> 添加密钥
              </Button>
            </div>

            {/* 添加密钥弹窗 */}
            {showAddKey && (
              <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm" onClick={() => setShowAddKey(false)}>
                <div className="w-full max-w-lg mx-4 rounded-2xl border border-border bg-card shadow-xl" onClick={e => e.stopPropagation()}>
                  <div className="flex items-center justify-between p-5 border-b border-border/50">
                    <h3 className="text-sm font-semibold">为 {endpoint.name} 添加密钥</h3>
                    <button onClick={() => setShowAddKey(false)} className="text-muted-foreground hover:text-foreground transition-colors">
                      <X className="h-4 w-4" />
                    </button>
                  </div>
                  <div className="p-5">
                    <textarea
                      placeholder="输入密钥，每行一个"
                      value={newKeysText}
                      onChange={e => setNewKeysText(e.target.value)}
                      rows={8}
                      className="w-full rounded-xl border border-input bg-background px-3 py-2 text-sm font-mono transition-all placeholder:text-muted-foreground focus-visible:outline-none focus-visible:border-primary/40 focus-visible:ring-2 focus-visible:ring-primary/10 resize-y"
                    />
                  </div>
                  <div className="flex items-center justify-between p-5 pt-0">
                    <label className="inline-flex items-center gap-1.5 h-9 px-3 rounded-lg border border-border bg-background text-sm font-medium text-muted-foreground hover:text-foreground hover:bg-accent transition-colors cursor-pointer">
                      上传文件
                      <input type="file" accept=".txt,.csv,.key" className="hidden" onChange={handleFileUpload} />
                    </label>
                    <div className="flex gap-2">
                      <Button size="sm" variant="outline" onClick={() => setShowAddKey(false)}>取消</Button>
                      <Button size="sm" onClick={handleAddKeys} disabled={adding || !newKeysText.trim()}>
                        {adding ? <Loader2 className="h-4 w-4 animate-spin" /> : null}
                        添加
                      </Button>
                    </div>
                  </div>
                </div>
              </div>
            )}

            {/* 密钥列表 */}
            {keys.length === 0 ? (
              <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
                <div className="flex h-12 w-12 items-center justify-center rounded-2xl bg-muted mb-3">
                  <KeyRound className="h-6 w-6 opacity-40" />
                </div>
                <p className="text-sm">暂无密钥，点击「添加密钥」开始</p>
              </div>
            ) : (
              <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
                {keys.map(key => (
                  <KeyCard
                    key={key.id}
                    apiKey={key}
                    isTesting={testingKeys.has(key.id)}
                    testResult={testResults[key.id]}
                    isRevealed={revealedKeys.has(key.id)}
                    onToggle={() => handleToggleKey(key.id, key.is_active)}
                    onDelete={() => handleDeleteKey(key.id)}
                    onTest={() => handleTestKey(key.id)}
                    onCopy={() => handleCopyKey(key.id)}
                    onToggleReveal={() => toggleRevealKey(key.id)}
                  />
                ))}
              </div>
            )}
          </div>
        )}
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  新增端点表单                                                       */
/* ------------------------------------------------------------------ */

function AddEndpointForm({ onAdded, onCancel }: { onAdded: () => void; onCancel: () => void }) {
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [adding, setAdding] = useState(false);

  async function handleSubmit() {
    if (!name.trim() || !baseUrl.trim()) return;
    setAdding(true);
    try {
      await api.addEndpoint({ name: name.trim(), base_url: baseUrl.trim() });
      onAdded();
    } catch (e) {
      console.error('添加端点失败:', e);
    } finally {
      setAdding(false);
    }
  }

  return (
    <Card className="border-border/50">
      <CardContent className="p-5 space-y-3">
        <p className="text-sm font-semibold">新增上游端点</p>
        <div className="flex flex-col gap-3 sm:flex-row">
          <Input
            placeholder="端点名称 (例如: OpenAI 主力)"
            value={name}
            onChange={e => setName(e.target.value)}
            className="sm:w-56"
          />
          <Input
            placeholder="https://api.example.com"
            value={baseUrl}
            onChange={e => setBaseUrl(e.target.value)}
            className="flex-1"
          />
          <div className="flex gap-2">
            <Button size="sm" onClick={handleSubmit} disabled={adding || !name.trim() || !baseUrl.trim()}>
              {adding ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
              确认添加
            </Button>
            <Button size="sm" variant="outline" onClick={onCancel}>取消</Button>
          </div>
        </div>
        <p className="text-xs text-muted-foreground">填写 OpenAI 兼容 API 的服务端地址，不要以斜杠结尾</p>
      </CardContent>
    </Card>
  );
}

/* ------------------------------------------------------------------ */
/*  主组件                                                             */
/* ------------------------------------------------------------------ */

export function ChannelManager() {
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [showAddEndpoint, setShowAddEndpoint] = useState(false);

  /* 加载所有数据 */
  const refresh = useCallback(() => {
    Promise.all([
      api.listEndpoints(),
      api.listApiKeys(),
    ]).then(([ep, ak]) => {
      setEndpoints(ep);
      setKeys(ak);
    }).catch(console.error).finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
    const timer = setInterval(refresh, 30000);
    return () => clearInterval(timer);
  }, [refresh]);

  /* 切换折叠 */
  function toggleExpand(id: string) {
    setExpandedIds(prev => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  }

  /* 按 endpoint 分组密钥 */
  function keysForEndpoint(endpointId: string): ApiKey[] {
    return keys.filter(k => k.endpoint_id === endpointId);
  }

  /* 汇总 */
  const totalKeys = keys.length;
  const activeKeys = keys.filter(k => k.is_active).length;
  const totalRequests = keys.reduce((sum, k) => sum + k.total_requests, 0);
  const activeEndpoints = endpoints.filter(e => e.is_active).length;

  return (
    <div className="space-y-6">
      {/* 页头 */}
      <div>
        <h2 className="text-[clamp(28px,4vw,38px)] font-semibold leading-[1.08] tracking-tight">
          渠道管理
        </h2>
        <p className="mt-2 text-muted-foreground text-[15px] leading-relaxed">
          管理上游 API 端点和密钥池
        </p>
      </div>

      {/* 汇总统计 */}
      <div className="flex flex-wrap items-center gap-6 text-sm text-muted-foreground">
        <span>端点: <strong className="text-foreground">{endpoints.length}</strong> (活跃 <span className="text-emerald-500 font-semibold">{activeEndpoints}</span>)</span>
        <span>密钥: <strong className="text-foreground">{totalKeys}</strong> (活跃 <span className="text-emerald-500 font-semibold">{activeKeys}</span>)</span>
        <span>总请求: <strong className="text-foreground">{totalRequests.toLocaleString()}</strong></span>
      </div>

      {/* 添加端点按钮 */}
      <div className="flex items-center gap-3">
        <Button size="sm" onClick={() => setShowAddEndpoint(!showAddEndpoint)}>
          <Plus className="h-4 w-4" /> 添加端点
        </Button>
      </div>

      {/* 添加端点表单 */}
      {showAddEndpoint && (
        <AddEndpointForm
          onAdded={() => { setShowAddEndpoint(false); refresh(); }}
          onCancel={() => setShowAddEndpoint(false)}
        />
      )}

      {/* 加载状态 */}
      {loading && endpoints.length === 0 && (
        <div className="flex items-center justify-center py-12 text-muted-foreground">
          <Loader2 className="h-5 w-5 animate-spin mr-3" /> 加载中...
        </div>
      )}

      {/* 空状态 */}
      {!loading && endpoints.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
          <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted mb-4">
            <Globe className="h-8 w-8 opacity-40" />
          </div>
          <p className="text-lg font-semibold text-foreground">暂无上游端点</p>
          <p className="text-sm mt-1">点击「添加端点」开始配置上游 API 服务</p>
        </div>
      )}

      {/* 端点列表 */}
      <div className="space-y-4">
        {endpoints.map(ep => (
          <EndpointCard
            key={ep.id}
            endpoint={ep}
            keys={keysForEndpoint(ep.id)}
            expanded={expandedIds.has(ep.id)}
            onToggleExpand={() => toggleExpand(ep.id)}
            onRefresh={refresh}
          />
        ))}
      </div>
    </div>
  );
}
