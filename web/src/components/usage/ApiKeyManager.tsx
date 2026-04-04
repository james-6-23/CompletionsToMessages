import { useState, useEffect } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { Input } from '@/components/ui/input';
import { useApiKeys } from '@/hooks/use-usage';
import { api } from '@/lib/api';
import type { ApiKey } from '@/types/usage';
import { Plus, Trash2, Eye, EyeOff, Copy, FlaskConical, Check, X, Loader2, KeyRound, Globe, Save, Shield, RefreshCw } from 'lucide-react';
import { fmtTimestamp } from './format';

function UpstreamUrlConfig() {
  const [url, setUrl] = useState('');
  const [saved, setSaved] = useState(false);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getUpstreamUrl().then(r => { setUrl(r.base_url || ''); setLoading(false); }).catch(() => setLoading(false));
  }, []);

  async function handleSave() {
    if (!url.trim()) return;
    try {
      await api.setUpstreamUrl(url.trim());
      setSaved(true);
      setTimeout(() => setSaved(false), 2000);
    } catch (e) {
      console.error('保存失败:', e);
    }
  }

  return (
    <Card className="border-border/50">
      <CardContent className="p-5">
        <div className="flex items-center gap-2 mb-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-blue-500/10">
            <Globe className="h-4 w-4 text-blue-500" />
          </div>
          <span className="text-sm font-semibold">上游端点地址</span>
        </div>
        <div className="flex gap-2">
          <Input
            placeholder="https://api.example.com"
            value={url}
            onChange={e => setUrl(e.target.value)}
            className="flex-1"
            disabled={loading}
          />
          <Button size="sm" onClick={handleSave} disabled={!url.trim() || loading}>
            {saved ? <Check className="h-4 w-4 text-emerald-500" /> : <Save className="h-4 w-4" />}
            {saved ? '已保存' : '保存'}
          </Button>
        </div>
        <p className="text-xs text-muted-foreground mt-2">填写 OpenAI 兼容 API 的服务端地址，不要以斜杠结尾</p>
      </CardContent>
    </Card>
  );
}

function AuthTokenConfig() {
  const [tokenMasked, setTokenMasked] = useState<string | null>(null);
  const [hasToken, setHasToken] = useState(false);
  const [newToken, setNewToken] = useState('');
  const [showInput, setShowInput] = useState(false);
  const [generatedToken, setGeneratedToken] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    api.getAuthToken().then(r => {
      setHasToken(r.has_token);
      setTokenMasked(r.token_masked);
      setLoading(false);
    }).catch(() => setLoading(false));
  }, []);

  async function handleGenerate() {
    try {
      const r = await api.setAuthToken();
      setGeneratedToken(r.token);
      setHasToken(true);
      setTokenMasked(r.token.length > 8 ? `${r.token.slice(0, 4)}...${r.token.slice(-4)}` : '****');
      setShowInput(false);
    } catch (e) {
      console.error('生成失败:', e);
    }
  }

  async function handleSetCustom() {
    if (!newToken.trim()) return;
    try {
      const r = await api.setAuthToken(newToken.trim());
      setGeneratedToken(r.token);
      setHasToken(true);
      setTokenMasked(r.token.length > 8 ? `${r.token.slice(0, 4)}...${r.token.slice(-4)}` : '****');
      setNewToken('');
      setShowInput(false);
    } catch (e) {
      console.error('设置失败:', e);
    }
  }

  function handleCopy(text: string) {
    navigator.clipboard.writeText(text).catch(() => {});
  }

  return (
    <Card className="border-border/50">
      <CardContent className="p-5">
        <div className="flex items-center gap-2 mb-3">
          <div className="flex h-8 w-8 items-center justify-center rounded-lg bg-emerald-500/10">
            <Shield className="h-4 w-4 text-emerald-500" />
          </div>
          <span className="text-sm font-semibold">入站认证 Token</span>
          {hasToken && <Badge variant="outline" className="text-emerald-600 dark:text-emerald-400 border-emerald-500/30 text-xs">已配置</Badge>}
          {!hasToken && !loading && <Badge variant="outline" className="text-amber-500 border-amber-500/30 text-xs">未配置</Badge>}
        </div>

        {hasToken && tokenMasked && !generatedToken && (
          <div className="flex items-center gap-2 text-sm">
            <code className="bg-muted px-2.5 py-1 rounded-lg font-mono text-sm">{tokenMasked}</code>
            <Button variant="ghost" size="sm" className="h-7 text-xs" onClick={() => setShowInput(true)}>
              <RefreshCw className="h-3 w-3" /> 重新生成
            </Button>
          </div>
        )}

        {generatedToken && (
          <div className="space-y-2">
            <div className="flex items-center gap-2">
              <code className="bg-emerald-500/10 text-emerald-600 dark:text-emerald-400 px-3 py-1.5 rounded-lg font-mono text-sm flex-1 break-all">{generatedToken}</code>
              <Button variant="ghost" size="icon" className="h-8 w-8 shrink-0" onClick={() => handleCopy(generatedToken)}>
                <Copy className="h-3.5 w-3.5" />
              </Button>
            </div>
            <p className="text-xs text-amber-500">请立即复制此 Token，刷新后将不再显示完整值。Claude Code 客户端的 ANTHROPIC_API_KEY 填此值。</p>
          </div>
        )}

        {!hasToken && !showInput && !generatedToken && (
          <div className="flex gap-2">
            <Button size="sm" onClick={handleGenerate}>
              <RefreshCw className="h-4 w-4" /> 自动生成
            </Button>
            <Button size="sm" variant="outline" onClick={() => setShowInput(true)}>自定义</Button>
          </div>
        )}

        {showInput && (
          <div className="flex gap-2 mt-2">
            <Input
              placeholder="输入自定义 Token 或留空自动生成"
              value={newToken}
              onChange={e => setNewToken(e.target.value)}
              className="flex-1"
            />
            <Button size="sm" onClick={newToken.trim() ? handleSetCustom : handleGenerate}>
              {newToken.trim() ? '确认' : '自动生成'}
            </Button>
            <Button size="sm" variant="outline" onClick={() => setShowInput(false)}>取消</Button>
          </div>
        )}

        <p className="text-xs text-muted-foreground mt-2">
          Claude Code 客户端设置 <code className="bg-muted px-1.5 py-0.5 rounded text-xs font-mono">ANTHROPIC_API_KEY</code> 为此 Token
        </p>
      </CardContent>
    </Card>
  );
}

export function ApiKeyManager() {
  const { data: keys, loading, refresh } = useApiKeys(30000);
  const [search, setSearch] = useState('');
  const [statusFilter, setStatusFilter] = useState<'all' | 'active' | 'inactive'>('all');
  const [showAddForm, setShowAddForm] = useState(false);
  const [newKey, setNewKey] = useState('');
  const [newLabel, setNewLabel] = useState('');
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());
  const [testingKeys, setTestingKeys] = useState<Set<string>>(new Set());
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});

  const filtered = keys.filter(k => {
    if (statusFilter === 'active' && !k.is_active) return false;
    if (statusFilter === 'inactive' && k.is_active) return false;
    if (search && !k.api_key_masked.toLowerCase().includes(search.toLowerCase()) && !k.label.toLowerCase().includes(search.toLowerCase())) return false;
    return true;
  });

  const totalKeys = keys.length;
  const activeKeys = keys.filter(k => k.is_active).length;
  const totalRequests = keys.reduce((sum, k) => sum + k.total_requests, 0);

  async function handleAdd() {
    if (!newKey.trim()) return;
    try {
      await api.addApiKey({ api_key: newKey.trim(), label: newLabel.trim() });
      setNewKey('');
      setNewLabel('');
      setShowAddForm(false);
      refresh();
    } catch (e) {
      console.error('添加失败:', e);
    }
  }

  async function handleDelete(id: string) {
    if (!confirm('确定删除此密钥？')) return;
    try {
      await api.deleteApiKey(id);
      refresh();
    } catch (e) {
      console.error('删除失败:', e);
    }
  }

  async function handleToggle(id: string, currentActive: boolean) {
    try {
      await api.toggleApiKey(id, !currentActive);
      refresh();
    } catch (e) {
      console.error('切换状态失败:', e);
    }
  }

  async function handleTest(id: string) {
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

  function handleCopy(masked: string) {
    navigator.clipboard.writeText(masked).catch(() => {});
  }

  function toggleReveal(id: string) {
    setRevealedKeys(prev => {
      const s = new Set(prev);
      if (s.has(id)) s.delete(id); else s.add(id);
      return s;
    });
  }

  return (
    <div className="space-y-6">
      {/* 上游端点配置 */}
      <UpstreamUrlConfig />

      {/* 入站认证 Token */}
      <AuthTokenConfig />

      {/* 汇总统计 */}
      <div className="flex items-center gap-6 text-sm text-muted-foreground">
        <span>密钥数量: <strong className="text-foreground">{totalKeys}</strong> (活跃 <span className="text-emerald-500 font-semibold">{activeKeys}</span>)</span>
        <span>总请求: <strong className="text-foreground">{totalRequests.toLocaleString()}</strong></span>
      </div>

      {/* 工具栏 */}
      <div className="flex flex-wrap items-center gap-3">
        <Button size="sm" onClick={() => setShowAddForm(!showAddForm)}>
          <Plus className="h-4 w-4" /> 添加密钥
        </Button>

        <select
          className="h-9 rounded-xl border border-input bg-background px-3 text-sm transition-colors focus:border-primary/40 focus:ring-2 focus:ring-primary/10 focus:outline-none"
          value={statusFilter}
          onChange={e => setStatusFilter(e.target.value as 'all' | 'active' | 'inactive')}
        >
          <option value="all">全部</option>
          <option value="active">有效</option>
          <option value="inactive">失效</option>
        </select>

        <Input
          placeholder="Key 搜索..."
          className="h-9 w-48"
          value={search}
          onChange={e => setSearch(e.target.value)}
        />
      </div>

      {/* 添加密钥表单 */}
      {showAddForm && (
        <Card className="border-border/50">
          <CardContent className="p-5 space-y-3">
            <div className="flex flex-col gap-3 sm:flex-row">
              <Input
                placeholder="API Key (sk-...)"
                value={newKey}
                onChange={e => setNewKey(e.target.value)}
                className="flex-1"
              />
              <Input
                placeholder="标签 (可选)"
                value={newLabel}
                onChange={e => setNewLabel(e.target.value)}
                className="w-48"
              />
              <div className="flex gap-2">
                <Button size="sm" onClick={handleAdd} disabled={!newKey.trim()}>确认添加</Button>
                <Button size="sm" variant="outline" onClick={() => setShowAddForm(false)}>取消</Button>
              </div>
            </div>
          </CardContent>
        </Card>
      )}

      {/* 加载状态 */}
      {loading && keys.length === 0 && (
        <div className="flex items-center justify-center py-12 text-muted-foreground">
          <div className="spinner mr-3" /> 加载中...
        </div>
      )}

      {/* 空状态 */}
      {!loading && keys.length === 0 && (
        <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
          <div className="flex h-16 w-16 items-center justify-center rounded-2xl bg-muted mb-4">
            <KeyRound className="h-8 w-8 opacity-40" />
          </div>
          <p className="text-lg font-semibold text-foreground">暂无密钥</p>
          <p className="text-sm mt-1">点击「添加密钥」开始管理 API Key</p>
        </div>
      )}

      {/* 密钥卡片网格 */}
      <div className="grid gap-4 sm:grid-cols-2 lg:grid-cols-3">
        {filtered.map(key => (
          <KeyCard
            key={key.id}
            apiKey={key}
            isTesting={testingKeys.has(key.id)}
            testResult={testResults[key.id]}
            isRevealed={revealedKeys.has(key.id)}
            onToggle={() => handleToggle(key.id, key.is_active)}
            onDelete={() => handleDelete(key.id)}
            onTest={() => handleTest(key.id)}
            onCopy={() => handleCopy(key.api_key_masked)}
            onToggleReveal={() => toggleReveal(key.id)}
          />
        ))}
      </div>
    </div>
  );
}

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
