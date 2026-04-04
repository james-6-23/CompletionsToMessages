import { useState, useEffect, useCallback } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Select, SelectTrigger, SelectContent, SelectItem, SelectValue } from '@/components/ui/select';
import { api } from '@/lib/api';
import { copyToClipboard } from '@/lib/utils';
import { toast } from '@/components/Toast';
import type { ApiKey, Endpoint } from '@/types/usage';
import {
  Plus, Trash2, Eye, EyeOff, Copy, FlaskConical, Check, X,
  Loader2, KeyRound, Globe, Pencil, Zap, Files, Wifi,
  Search, MoreHorizontal, MinusCircle, Download, RotateCcw,
} from 'lucide-react';
import { fmtTimestamp } from './format';

/* ------------------------------------------------------------------ */
/*  工具函数：根据端点名称生成颜色                                        */
/* ------------------------------------------------------------------ */

const AVATAR_COLORS = [
  'bg-purple-500', 'bg-blue-500', 'bg-green-500', 'bg-orange-500',
  'bg-pink-500', 'bg-indigo-500', 'bg-teal-500', 'bg-red-500',
  'bg-yellow-500', 'bg-cyan-500',
];

function getAvatarColor(name: string): string {
  let hash = 0;
  for (let i = 0; i < name.length; i++) hash = name.charCodeAt(i) + ((hash << 5) - hash);
  return AVATAR_COLORS[Math.abs(hash) % AVATAR_COLORS.length];
}

function getAvatarLetter(name: string): string {
  return name.charAt(0).toUpperCase();
}

/* ------------------------------------------------------------------ */
/*  左侧渠道列表项                                                      */
/* ------------------------------------------------------------------ */

/* 从 website_url 提取 favicon URL（Google S2 服务） */
function getFaviconUrl(websiteUrl: string): string | null {
  if (!websiteUrl) return null;
  try {
    const url = websiteUrl.startsWith('http') ? websiteUrl : `https://${websiteUrl}`;
    const origin = new URL(url).origin;
    return `https://www.google.com/s2/favicons?domain=${origin}&sz=32`;
  } catch {
    return null;
  }
}

/* 获取渠道图标 URL：优先 logo_url，其次从 website_url 提取 favicon */
function getIconUrl(endpoint: Endpoint): string | null {
  if (endpoint.logo_url) return endpoint.logo_url;
  return getFaviconUrl(endpoint.website_url);
}

function ChannelListItem({
  endpoint,
  isSelected,
  onClick,
}: {
  endpoint: Endpoint;
  isSelected: boolean;
  onClick: () => void;
}) {
  const color = getAvatarColor(endpoint.name);
  const letter = getAvatarLetter(endpoint.name);
  const tag = '#' + endpoint.name.toLowerCase().replace(/\s+/g, '_');
  const iconUrl = getIconUrl(endpoint);
  const [iconOk, setIconOk] = useState(!!iconUrl);

  return (
    <div
      className={`flex items-center gap-3 px-3 py-2.5 rounded-xl cursor-pointer transition-all duration-150 ${
        isSelected
          ? 'bg-primary/8 border border-primary/20'
          : 'hover:bg-muted/60 border border-transparent'
      }`}
      onClick={onClick}
    >
      {/* 头像：有图标则显示，否则显示字母 */}
      <div className={`flex h-9 w-9 shrink-0 items-center justify-center rounded-xl overflow-hidden ${iconUrl && iconOk ? 'bg-white dark:bg-muted border border-border/30' : `${color} text-white font-bold text-sm`}`}>
        {iconUrl && iconOk ? (
          <img
            src={iconUrl}
            alt=""
            className="h-5 w-5 object-contain"
            onError={() => setIconOk(false)}
          />
        ) : (
          letter
        )}
      </div>

      {/* 名称 + 标签 */}
      <div className="flex-1 min-w-0">
        <p className={`text-sm font-semibold truncate ${isSelected ? 'text-primary' : 'text-foreground'}`}>
          {endpoint.name}
        </p>
        <div className="flex items-center gap-1.5 mt-0.5">
          <span className="text-xs px-1.5 py-0.5 rounded-full font-medium bg-emerald-500/10 text-emerald-600 dark:text-emerald-400">
            openai
          </span>
          <span className="text-xs truncate text-muted-foreground">
            {tag}
          </span>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  密钥卡片（右侧网格）                                                */
/* ------------------------------------------------------------------ */

function KeyCard({
  apiKey,
  isTesting,
  testResult,
  isRevealed,
  onToggle,
  onDelete,
  onTest,
  onCopy,
  onToggleReveal,
}: {
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
    <div className={`rounded-xl border p-3 space-y-2 transition-all duration-200 hover:shadow-sm ${
      apiKey.is_active
        ? 'border-emerald-200 bg-emerald-50/50 dark:border-emerald-500/20 dark:bg-emerald-500/[0.04]'
        : 'border-red-200 bg-red-50/50 dark:border-red-500/20 dark:bg-red-500/[0.04]'
    }`}>
      {/* 头部：状态 + 掩码key + 操作图标 */}
      <div className="flex items-center gap-2">
        {/* 状态 */}
        <div
          className={`flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium cursor-pointer shrink-0 ${
            apiKey.is_active
              ? 'bg-emerald-500/10 text-emerald-600 dark:text-emerald-400'
              : 'bg-red-500/10 text-red-500'
          }`}
          onClick={onToggle}
        >
          <div className={`w-1.5 h-1.5 rounded-full ${apiKey.is_active ? 'bg-emerald-500' : 'bg-red-400'}`} />
          {apiKey.is_active ? '有效' : '失效'}
        </div>

        {/* 掩码Key */}
        <code className="flex-1 text-xs font-mono text-foreground truncate min-w-0">
          {apiKey.api_key_masked}
        </code>

        {/* 图标操作组 */}
        <div className="flex items-center gap-0.5 shrink-0">
          <button
            className="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
            onClick={onToggleReveal}
            title="显示/隐藏"
          >
            {isRevealed ? <EyeOff className="h-3.5 w-3.5" /> : <Eye className="h-3.5 w-3.5" />}
          </button>
          <button
            className="p-1 rounded-md text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
            onClick={onCopy}
            title="复制"
          >
            <Copy className="h-3.5 w-3.5" />
          </button>
        </div>
      </div>

      {/* 标签 */}
      {apiKey.label && (
        <p className="text-xs text-muted-foreground truncate">{apiKey.label}</p>
      )}

      {/* 统计：请求 / 失败 / 未使用 */}
      <div className="flex items-center gap-3 text-xs text-muted-foreground">
        <span>
          请求 <strong className="text-foreground">{apiKey.total_requests}</strong>
        </span>
        <span>
          失败 <strong className={apiKey.failed_requests > 0 ? 'text-red-500' : 'text-foreground'}>
            {apiKey.failed_requests}
          </strong>
        </span>
        {apiKey.last_used_at ? (
          <span className="truncate">最近 {fmtTimestamp(apiKey.last_used_at)}</span>
        ) : (
          <span>未使用</span>
        )}
      </div>

      {/* 操作行：测试 + 删除 */}
      <div className="flex items-center justify-between pt-1 border-t border-border/40">
        <button
          className="flex items-center gap-1 text-xs text-blue-500 hover:text-blue-600 font-medium transition-colors disabled:opacity-50"
          onClick={onTest}
          disabled={isTesting}
        >
          {isTesting
            ? <Loader2 className="h-3 w-3 animate-spin" />
            : <FlaskConical className="h-3 w-3" />}
          测试
          {testResult === true && <Check className="h-3 w-3 text-emerald-500" />}
          {testResult === false && <X className="h-3 w-3 text-red-500" />}
        </button>
        <button
          className="flex items-center gap-1 text-xs text-red-500 hover:text-red-600 font-medium transition-colors"
          onClick={onDelete}
        >
          <Trash2 className="h-3 w-3" /> 删除
        </button>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  同步模型按钮                                                        */
/* ------------------------------------------------------------------ */

function SyncModelsButton({ endpointId, onSynced }: { endpointId: string; onSynced: () => void }) {
  const [syncing, setSyncing] = useState(false);

  async function handleSync() {
    setSyncing(true);
    try {
      const result = await api.syncEndpointModels(endpointId);
      toast(`已同步 ${result.count} 个模型`);
      onSynced();
    } catch (e) {
      console.error('同步模型列表失败:', e);
      toast('同步失败', 'error');
    } finally {
      setSyncing(false);
    }
  }

  return (
    <button
      onClick={handleSync}
      disabled={syncing}
      className="inline-flex items-center gap-1 text-xs text-primary hover:underline disabled:opacity-50"
    >
      {syncing ? <Loader2 className="h-3 w-3 animate-spin" /> : null}
      {syncing ? '同步中...' : '从上游同步'}
    </button>
  );
}

/* ------------------------------------------------------------------ */
/*  右侧详情面板                                                        */
/* ------------------------------------------------------------------ */

function ChannelDetailPanel({
  endpoint,
  keys,
  onRefresh,
}: {
  endpoint: Endpoint;
  keys: ApiKey[];
  onRefresh: () => void;
}) {
  /* 编辑弹窗 */
  const [showEdit, setShowEdit] = useState(false);
  const [editName, setEditName] = useState(endpoint.name);
  const [editUrl, setEditUrl] = useState(endpoint.base_url);
  const [editWebsite, setEditWebsite] = useState(endpoint.website_url || '');
  const [editLogo, setEditLogo] = useState(endpoint.logo_url || '');
  const [editProxy, setEditProxy] = useState(endpoint.proxy_url || '');
  const [saving, setSaving] = useState(false);
  const [testingProxy, setTestingProxy] = useState(false);

  /* 代理出口 IP 展示 */
  const [proxyIp, setProxyIp] = useState<string | null>(null);
  const [testingProxyIp, setTestingProxyIp] = useState(false);

  useEffect(() => {
    setProxyIp(null);
    if (endpoint.proxy_url) {
      setTestingProxyIp(true);
      api.testProxy(endpoint.proxy_url).then(res => {
        if (res.ok && res.ip) setProxyIp(res.ip);
      }).catch(() => {}).finally(() => setTestingProxyIp(false));
    }
  }, [endpoint.id, endpoint.proxy_url]);

  /* 添加密钥弹窗 */
  const [showAddKey, setShowAddKey] = useState(false);
  const [newKeysText, setNewKeysText] = useState('');
  const [adding, setAdding] = useState(false);

  /* Key 交互状态 */
  const [revealedKeys, setRevealedKeys] = useState<Set<string>>(new Set());
  const [testingKeys, setTestingKeys] = useState<Set<string>>(new Set());
  const [testResults, setTestResults] = useState<Record<string, boolean | null>>({});

  /* 模型列表（测试用）— 持久化到服务端数据库 */
  const [models, setModels] = useState<string[]>([]);
  const [testModel, setTestModel] = useState('');
  const [loadingModels, setLoadingModels] = useState(false);

  // 初始化：从服务端加载缓存的模型列表和测试模型
  useEffect(() => {
    api.getSetting(`models_${endpoint.id}`).then(r => {
      if (r.value) try { setModels(JSON.parse(r.value)); } catch { /* ignore */ }
    }).catch(() => {});
    api.getSetting(`testModel_${endpoint.id}`).then(r => {
      if (r.value) setTestModel(r.value);
    }).catch(() => {});
  }, [endpoint.id]);

  /* Key 搜索过滤 + 分页 */
  const [searchKey, setSearchKey] = useState('');
  const [filterStatus, setFilterStatus] = useState<'all' | 'valid' | 'invalid'>('all');
  const [keyPage, setKeyPage] = useState(1);
  const KEY_PAGE_SIZE = 30;

  /* 详情展开 */
  const [showDetails, setShowDetails] = useState(false);

  /* 当 endpoint 切换时，重置状态 */
  useEffect(() => {
    setShowEdit(false);
    setEditName(endpoint.name);
    setEditUrl(endpoint.base_url);
    setEditWebsite(endpoint.website_url || '');
    setEditLogo(endpoint.logo_url || '');
    setEditProxy(endpoint.proxy_url || '');
    setSearchKey('');
    setFilterStatus('all');
    setTestResults({});
    setRevealedKeys(new Set());
    setModels([]);
    setTestModel('');
  }, [endpoint.id]);

  async function fetchModels() {
    if (loadingModels || keys.length === 0) return;
    setLoadingModels(true);
    try {
      const resp = await api.getEndpointModels(endpoint.id);
      const ids = (resp.data || []).map((m: { id: string }) => m.id).sort();
      setModels(ids);
      api.setSetting(`models_${endpoint.id}`, JSON.stringify(ids)).catch(() => {});
      if (ids.length > 0 && !testModel) {
        setTestModel(ids[0]);
        api.setSetting(`testModel_${endpoint.id}`, ids[0]).catch(() => {});
      }
    } catch (e) {
      console.error('获取模型列表失败:', e);
    } finally {
      setLoadingModels(false);
    }
  }

  async function handleSaveEndpoint() {
    if (!editName.trim() || !editUrl.trim()) return;
    setSaving(true);
    try {
      await api.updateEndpoint(endpoint.id, {
        name: editName.trim(),
        base_url: editUrl.trim(),
        website_url: editWebsite.trim(),
        logo_url: editLogo.trim(),
        proxy_url: editProxy.trim(),
      });
      setShowEdit(false);
      onRefresh();
    } catch (e) {
      console.error('保存端点失败:', e);
    } finally {
      setSaving(false);
    }
  }

  async function handleDeleteEndpoint() {
    if (!confirm(`确定删除端点「${endpoint.name}」？关联的所有密钥也会被删除。`)) return;
    try {
      await api.deleteEndpoint(endpoint.id);
      onRefresh();
    } catch (e) {
      console.error('删除端点失败:', e);
    }
  }

  const [cloning, setCloning] = useState(false);
  async function handleCloneEndpoint() {
    setCloning(true);
    try {
      await api.addEndpoint({
        name: `${endpoint.name} copy`,
        base_url: endpoint.base_url,
        website_url: endpoint.website_url || undefined,
        logo_url: endpoint.logo_url || undefined,
        proxy_url: endpoint.proxy_url || undefined,
      });
      toast('渠道已复制');
      onRefresh();
    } catch (e) {
      console.error('复制渠道失败:', e);
      toast('复制渠道失败', 'error');
    } finally {
      setCloning(false);
    }
  }

  async function handleAddKeys() {
    const lines = newKeysText.split('\n').map(l => l.trim()).filter(l => l.length > 0);
    if (lines.length === 0) return;
    setAdding(true);
    try {
      const result = await api.batchAddApiKeys({ endpoint_id: endpoint.id, api_keys: lines });
      toast(`已添加 ${result.count} 个密钥`);
      setNewKeysText('');
      setShowAddKey(false);
      onRefresh();
    } catch (e) {
      console.error('添加密钥失败:', e);
      toast('添加密钥失败', 'error');
    } finally {
      setAdding(false);
    }
  }

  function handleFileUpload(e: React.ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0];
    if (!file) return;
    const reader = new FileReader();
    reader.onload = (ev) => {
      const text = ev.target?.result as string;
      if (text) setNewKeysText(prev => prev ? prev + '\n' + text.trim() : text.trim());
    };
    reader.readAsText(file);
    e.target.value = '';
  }

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
    const modelName = testModel || '默认模型';
    try {
      const result = await api.testApiKey(id, testModel || undefined);
      setTestResults(prev => ({ ...prev, [id]: result.valid }));
      if (result.valid) {
        toast(`测试通过 [${modelName}]`, 'success');
      } else {
        const detail = result.error
          ? `${result.status || 'err'}: ${result.error.slice(0, 150)}`
          : `状态码 ${result.status}`;
        toast(`测试失败 [${modelName}] — ${detail}`, 'error');
      }
    } catch {
      setTestResults(prev => ({ ...prev, [id]: false }));
      toast(`测试失败 [${modelName}] — 网络错误`, 'error');
    } finally {
      setTestingKeys(prev => { const s = new Set(prev); s.delete(id); return s; });
    }
  }

  /* 批量测试：并发 5 个一批，避免压垮上游 */
  const [batchTesting, setBatchTesting] = useState(false);
  async function handleBatchTest() {
    const activeKeys = keys.filter(k => k.is_active);
    if (activeKeys.length === 0) { toast('没有有效密钥可测试', 'error'); return; }
    const modelName = testModel || '默认模型';
    setBatchTesting(true);
    setTestResults({});
    let passed = 0;
    let failed = 0;
    const CONCURRENCY = 5;

    for (let i = 0; i < activeKeys.length; i += CONCURRENCY) {
      const batch = activeKeys.slice(i, i + CONCURRENCY);
      batch.forEach(k => setTestingKeys(prev => new Set(prev).add(k.id)));

      const results = await Promise.allSettled(
        batch.map(async k => {
          const result = await api.testApiKey(k.id, testModel || undefined);
          setTestResults(prev => ({ ...prev, [k.id]: result.valid }));
          return result.valid;
        })
      );

      batch.forEach(k => setTestingKeys(prev => { const s = new Set(prev); s.delete(k.id); return s; }));
      for (const r of results) {
        if (r.status === 'fulfilled' && r.value) passed++; else failed++;
      }
    }

    setBatchTesting(false);
    toast(`批量测试完成 [${modelName}]：${passed} 通过，${failed} 失败`, passed > 0 ? 'success' : 'error');
    onRefresh();
  }

  /* 更多操作菜单 */
  const [showMoreMenu, setShowMoreMenu] = useState(false);

  async function exportKeysToClipboard(status?: string) {
    setShowMoreMenu(false);
    try {
      const result = await api.exportKeys({ endpoint_id: endpoint.id, status: status || 'all' });
      if (result.keys.length === 0) { toast('没有可导出的密钥'); return; }
      copyToClipboard(result.keys.join('\n'));
      toast(`已复制 ${result.keys.length} 个密钥到剪贴板`);
    } catch { toast('导出失败', 'error'); }
  }

  async function batchDeleteKeys(status: string) {
    setShowMoreMenu(false);
    const label = status === 'all' ? '所有' : status === 'valid' ? '有效' : '无效';
    const count = status === 'all' ? keys.length : status === 'valid' ? keys.filter(k => k.is_active).length : keys.filter(k => !k.is_active).length;
    if (count === 0) { toast(`没有${label}密钥`); return; }
    if (!confirm(`确定清空 ${count} 个${label}密钥？此操作不可撤销！`)) return;
    try {
      const result = await api.batchDeleteApiKeysPost({ endpoint_id: endpoint.id, status });
      toast(`已删除 ${result.count} 个密钥`);
      setKeyPage(1);
      onRefresh();
    } catch { toast('删除失败', 'error'); }
  }

  async function restoreInvalidKeys() {
    setShowMoreMenu(false);
    try {
      const result = await api.batchRestoreKeys({ endpoint_id: endpoint.id });
      if (result.count === 0) { toast('没有失效密钥需要恢复'); return; }
      toast(`已恢复 ${result.count} 个密钥`);
      onRefresh();
    } catch { toast('恢复失败', 'error'); }
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

  /* 统计数据 */
  const totalKeys = keys.length;
  const req24h = keys.reduce((s, k) => s + k.total_requests, 0);
  const failedTotal = keys.reduce((s, k) => s + k.failed_requests, 0);
  const activeCount = keys.filter(k => k.is_active).length;

  /* 过滤密钥 */
  const filteredKeys = keys.filter(k => {
    const matchSearch = !searchKey || k.api_key_masked.toLowerCase().includes(searchKey.toLowerCase());
    const matchStatus =
      filterStatus === 'all' ||
      (filterStatus === 'valid' && k.is_active) ||
      (filterStatus === 'invalid' && !k.is_active);
    return matchSearch && matchStatus;
  });

  /* 分页 */
  const keyTotalPages = Math.ceil(filteredKeys.length / KEY_PAGE_SIZE);
  const safeKeyPage = Math.min(keyPage, Math.max(1, keyTotalPages));
  const pagedKeys = filteredKeys.slice((safeKeyPage - 1) * KEY_PAGE_SIZE, safeKeyPage * KEY_PAGE_SIZE);

  return (
    <div className="flex-1 flex flex-col min-h-0 overflow-y-auto">
      {/* 顶部：标题 + URL + 操作图标 */}
      <div className="flex items-center justify-between gap-4 p-6 border-b border-border/50">
        <div className="flex items-center gap-3 flex-wrap min-w-0">
          <h2 className="text-2xl font-bold tracking-tight">{endpoint.name}</h2>
          <span className="text-sm text-muted-foreground bg-muted/60 px-3 py-1 rounded-lg font-mono truncate max-w-sm">
            {endpoint.base_url}
          </span>
          {endpoint.website_url && (
            <a
              href={endpoint.website_url.startsWith('http') ? endpoint.website_url : `https://${endpoint.website_url}`}
              target="_blank"
              rel="noopener noreferrer"
              className="inline-flex items-center gap-1 text-xs text-primary hover:underline"
              title="官网"
            >
              <Globe className="h-3.5 w-3.5" />
              官网
            </a>
          )}
          {endpoint.proxy_url && (
            <span className="inline-flex items-center gap-1.5 text-xs text-muted-foreground bg-muted/60 px-2.5 py-1 rounded-lg font-mono">
              {testingProxyIp ? (
                <Loader2 className="h-3 w-3 animate-spin" />
              ) : proxyIp ? (
                <>{proxyIp}</>
              ) : (
                <>代理</>
              )}
              <button
                className="p-0.5 rounded hover:bg-muted transition-colors disabled:opacity-50"
                disabled={testingProxyIp}
                title="测试代理"
                onClick={async () => {
                  setTestingProxyIp(true);
                  try {
                    const res = await api.testProxy(endpoint.proxy_url);
                    if (res.ok) {
                      setProxyIp(res.ip || null);
                      toast(`延迟 ${res.latency_ms}ms｜${res.location}｜IP ${res.ip}`, 'success');
                    } else {
                      setProxyIp(null);
                      toast(res.error || '代理测试失败', 'error');
                    }
                  } catch {
                    setProxyIp(null);
                    toast('代理测试请求失败', 'error');
                  } finally {
                    setTestingProxyIp(false);
                  }
                }}
              >
                <Wifi className="h-3.5 w-3.5" />
              </button>
            </span>
          )}
        </div>

        {/* 操作图标 */}
        <div className="flex items-center gap-1 shrink-0">
          <button
            className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
            onClick={() => { copyToClipboard(endpoint.base_url); toast('已复制 URL', 'success'); }}
            title="复制 URL"
          >
            <Copy className="h-4 w-4" />
          </button>
          <button
            className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
            onClick={() => { setEditName(endpoint.name); setEditUrl(endpoint.base_url); setEditWebsite(endpoint.website_url || ''); setEditLogo(endpoint.logo_url || ''); setEditProxy(endpoint.proxy_url || ''); setShowEdit(true); }}
            title="编辑"
          >
            <Pencil className="h-4 w-4" />
          </button>
          <button
            className="p-2 rounded-lg text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors disabled:opacity-50"
            onClick={handleCloneEndpoint}
            disabled={cloning}
            title="复制渠道"
          >
            {cloning ? <Loader2 className="h-4 w-4 animate-spin" /> : <Files className="h-4 w-4" />}
          </button>
          <button
            className="p-2 rounded-lg text-red-400 hover:text-red-500 hover:bg-red-500/10 transition-colors"
            onClick={handleDeleteEndpoint}
            title="删除"
          >
            <Trash2 className="h-4 w-4" />
          </button>
        </div>
      </div>

      {/* 统计卡片行 */}
      <div className="grid grid-cols-4 gap-0 border-b border-border/50">
        {[
          { label: '密钥数量', value: totalKeys, sub: activeCount, subColor: 'text-emerald-500' },
          { label: '24小时请求', value: 0, sub: 0, subColor: 'text-red-400' },
          { label: '7天请求', value: req24h, sub: failedTotal, subColor: 'text-red-400' },
          { label: '30天请求', value: 0, sub: 0, subColor: 'text-red-400' },
        ].map((stat, i) => (
          <div key={i} className={`px-6 py-4 ${i < 3 ? 'border-r border-border/50' : ''}`}>
            <p className="text-xs text-muted-foreground mb-1">{stat.label}</p>
            <div className="flex items-baseline gap-2">
              <span className="text-xl font-bold text-foreground">{stat.value}</span>
              <span className={`text-xl font-bold ${stat.subColor}`}>{stat.sub}</span>
            </div>
          </div>
        ))}
      </div>

      {/* 详细信息（折叠） */}
      <div className="border-b border-border/50">
        <button
          className="flex items-center gap-2 px-6 py-3 text-sm text-muted-foreground hover:text-foreground transition-colors w-full text-left"
          onClick={() => setShowDetails(!showDetails)}
        >
          <span className="text-xs">{showDetails ? '▼' : '▶'}</span>
          详细信息
        </button>
        {showDetails && (
          <div className="px-6 pb-4 space-y-3 text-sm text-muted-foreground">
            <div className="flex gap-8">
              <span>活跃密钥: <strong className="text-emerald-500">{activeCount}</strong> / {totalKeys}</span>
              <span>总请求: <strong className="text-foreground">{req24h.toLocaleString()}</strong></span>
              <span>总失败: <strong className="text-red-500">{failedTotal}</strong></span>
            </div>

            {/* 支持的模型列表 */}
            <div className="space-y-1.5 pt-1">
              <div className="flex items-center gap-2">
                <span className="text-xs font-medium">支持的模型:</span>
                <SyncModelsButton endpointId={endpoint.id} onSynced={onRefresh} />
              </div>
              {endpoint.models.length > 0 ? (
                <div className="flex flex-wrap gap-1.5">
                  {endpoint.models.map(m => (
                    <span key={m} className="inline-flex items-center px-2.5 py-1 rounded-lg border border-border/50 bg-muted/40 text-xs font-semibold font-mono text-foreground">
                      {m}
                    </span>
                  ))}
                </div>
              ) : (
                <p className="text-xs text-muted-foreground">未同步（不限模型，所有请求都会路由到此渠道）</p>
              )}
            </div>

            {/* 测试模型选择 */}
            <div className="flex items-center gap-3 pt-1">
              <span className="text-xs font-medium text-muted-foreground shrink-0">测试模型:</span>
              <Select value={testModel || undefined} onValueChange={v => { setTestModel(v); api.setSetting(`testModel_${endpoint.id}`, v).catch(() => {}); }} onOpenChange={o => { if (o && models.length === 0) fetchModels(); }}>
                <SelectTrigger className="w-64 h-8 text-sm font-mono font-semibold">
                  <SelectValue placeholder={loadingModels ? '加载中...' : '选择模型'} />
                </SelectTrigger>
                <SelectContent className="max-h-[320px]">
                  {models.map(m => <SelectItem key={m} value={m} className="text-sm font-mono font-medium">{m}</SelectItem>)}
                  {models.length === 0 && !loadingModels && (
                    <div className="px-2 py-1.5 text-sm text-muted-foreground">暂无模型</div>
                  )}
                </SelectContent>
              </Select>
              <button onClick={() => { setModels([]); fetchModels(); }} className="text-xs text-primary hover:underline">
                {loadingModels ? '加载中...' : '刷新'}
              </button>
            </div>
          </div>
        )}
      </div>

      {/* 工具栏：添加 / 删除 / 筛选 / 搜索 */}
      <div className="flex items-center gap-3 px-6 py-4 border-b border-border/50 flex-wrap">
        {/* 添加密钥按钮 */}
        <Button
          size="sm"
          className="bg-emerald-500 hover:bg-emerald-600 text-white h-8 gap-1.5 rounded-full px-4 text-xs font-semibold"
          onClick={() => setShowAddKey(true)}
        >
          <Plus className="h-3.5 w-3.5" /> 添加密钥
        </Button>

        {/* 批量测试 */}
        <Button
          size="sm"
          variant="outline"
          className="border-blue-300 text-blue-500 hover:bg-blue-50 dark:hover:bg-blue-500/10 h-8 gap-1.5 rounded-full px-4 text-xs font-semibold"
          onClick={handleBatchTest}
          disabled={batchTesting || keys.filter(k => k.is_active).length === 0}
        >
          {batchTesting ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <FlaskConical className="h-3.5 w-3.5" />}
          {batchTesting ? '测试中...' : '批量测试'}
        </Button>

        {/* 删除失效密钥 */}
        <Button
          size="sm"
          variant="outline"
          className="border-red-300 text-red-500 hover:bg-red-50 dark:hover:bg-red-500/10 h-8 gap-1.5 rounded-full px-4 text-xs font-semibold"
          onClick={async () => {
            const invalidKeys = keys.filter(k => !k.is_active);
            if (invalidKeys.length === 0) { toast('没有失效密钥'); return; }
            if (!confirm(`确定删除 ${invalidKeys.length} 个失效密钥？`)) return;
            for (const k of invalidKeys) { await api.deleteApiKey(k.id).catch(() => {}); }
            toast(`已删除 ${invalidKeys.length} 个失效密钥`);
            onRefresh();
          }}
          disabled={keys.filter(k => !k.is_active).length === 0}
        >
          <MinusCircle className="h-3.5 w-3.5" /> 删除失效
        </Button>

        <div className="flex-1" />

        {/* 状态筛选 */}
        <Select value={filterStatus} onValueChange={v => { setFilterStatus(v as 'all' | 'valid' | 'invalid'); setKeyPage(1); }}>
          <SelectTrigger className="w-24 h-8 text-xs rounded-lg">
            <SelectValue />
          </SelectTrigger>
          <SelectContent>
            <SelectItem value="all" className="text-xs">全部</SelectItem>
            <SelectItem value="valid" className="text-xs">有效</SelectItem>
            <SelectItem value="invalid" className="text-xs">失效</SelectItem>
          </SelectContent>
        </Select>

        {/* 搜索 */}
        <div className="relative">
          <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
          <Input
            value={searchKey}
            onChange={e => { setSearchKey(e.target.value); setKeyPage(1); }}
            placeholder="Key 精确匹配"
            className="h-8 pl-8 pr-3 text-xs w-40 rounded-lg"
          />
        </div>
        <Button size="sm" className="h-8 text-xs px-3 rounded-lg">搜索</Button>
        <div className="relative">
          <button
            className="p-2 rounded-lg text-muted-foreground hover:bg-muted/60 transition-colors"
            onClick={() => setShowMoreMenu(!showMoreMenu)}
          >
            <MoreHorizontal className="h-4 w-4" />
          </button>
          {showMoreMenu && (
            <>
              <div className="fixed inset-0 z-40" onClick={() => setShowMoreMenu(false)} />
              <div className="absolute right-0 top-full mt-1 z-50 w-48 rounded-xl border border-border bg-card shadow-xl py-1 text-xs">
                <button onClick={() => exportKeysToClipboard('all')} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <Download className="h-3.5 w-3.5" /> 导出所有密钥
                </button>
                <button onClick={() => exportKeysToClipboard('valid')} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <Download className="h-3.5 w-3.5" /> 导出有效密钥
                </button>
                <button onClick={() => exportKeysToClipboard('invalid')} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <Download className="h-3.5 w-3.5" /> 导出无效密钥
                </button>
                <div className="border-t border-border/50 my-1" />
                <button onClick={restoreInvalidKeys} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <RotateCcw className="h-3.5 w-3.5" /> 恢复所有无效密钥
                </button>
                <div className="border-t border-border/50 my-1" />
                <button onClick={() => batchDeleteKeys('invalid')} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2 text-red-500">
                  <Trash2 className="h-3.5 w-3.5" /> 清空所有无效密钥
                </button>
                <button onClick={() => batchDeleteKeys('all')} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2 text-red-600 font-semibold">
                  <Trash2 className="h-3.5 w-3.5" /> 清空所有密钥
                </button>
                <div className="border-t border-border/50 my-1" />
                <button onClick={() => { setShowMoreMenu(false); handleBatchTest(); }} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <FlaskConical className="h-3.5 w-3.5" /> 验证所有密钥
                </button>
                <button onClick={() => { setShowMoreMenu(false); /* 只测有效 */ const orig = handleBatchTest; orig(); }} className="w-full px-3 py-2 text-left hover:bg-muted/60 flex items-center gap-2">
                  <FlaskConical className="h-3.5 w-3.5" /> 验证有效密钥
                </button>
              </div>
            </>
          )}
        </div>
      </div>

      {/* 密钥网格 */}
      <div className="flex-1 p-6">
        {filteredKeys.length === 0 ? (
          <div className="flex flex-col items-center justify-center py-16 text-muted-foreground">
            <div className="flex h-14 w-14 items-center justify-center rounded-2xl bg-muted mb-4">
              <KeyRound className="h-7 w-7 opacity-40" />
            </div>
            <p className="text-sm font-medium text-foreground">
              {keys.length === 0 ? '暂无密钥，点击「添加密钥」开始' : '无匹配的密钥'}
            </p>
          </div>
        ) : (
          <>
            <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
              {pagedKeys.map(key => (
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
            {/* 分页 */}
            {keyTotalPages > 1 && (
              <div className="flex items-center justify-between mt-4 pt-4 border-t border-border/50">
                <span className="text-xs text-muted-foreground">
                  共 {filteredKeys.length} 个密钥，第 {safeKeyPage}/{keyTotalPages} 页
                </span>
                <div className="flex items-center gap-1">
                  <button
                    disabled={safeKeyPage <= 1}
                    onClick={() => setKeyPage(p => Math.max(1, p - 1))}
                    className="h-7 px-2 text-xs rounded border border-border bg-background text-muted-foreground hover:text-foreground disabled:opacity-40 transition-colors"
                  >上一页</button>
                  <button
                    disabled={safeKeyPage >= keyTotalPages}
                    onClick={() => setKeyPage(p => p + 1)}
                    className="h-7 px-2 text-xs rounded border border-border bg-background text-muted-foreground hover:text-foreground disabled:opacity-40 transition-colors"
                  >下一页</button>
                </div>
              </div>
            )}
          </>
        )}
      </div>

      {/* 添加密钥弹窗 */}
      {showAddKey && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
          onClick={() => setShowAddKey(false)}
        >
          <div
            className="w-full max-w-lg mx-4 rounded-2xl border border-border bg-card shadow-xl"
            onClick={e => e.stopPropagation()}
          >
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

      {/* 编辑端点弹窗 */}
      {showEdit && (
        <div
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
          onClick={() => setShowEdit(false)}
        >
          <div
            className="w-full max-w-lg mx-4 rounded-2xl border border-border bg-card shadow-xl"
            onClick={e => e.stopPropagation()}
          >
            <div className="flex items-center justify-between px-6 py-4 border-b border-border/60">
              <h3 className="text-base font-semibold">编辑渠道</h3>
              <button onClick={() => setShowEdit(false)} className="flex items-center justify-center w-7 h-7 rounded-full text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors">
                <X className="h-4 w-4" />
              </button>
            </div>
            <div className="px-6 py-5 space-y-4">
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  渠道名称 <span className="text-red-500">*</span>
                </label>
                <Input
                  placeholder="例如: OpenAI 主力"
                  value={editName}
                  onChange={e => setEditName(e.target.value)}
                  className="flex-1 h-9"
                />
              </div>
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  上游地址 <span className="text-red-500">*</span>
                </label>
                <Input
                  placeholder="https://api.example.com"
                  value={editUrl}
                  onChange={e => setEditUrl(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  官网地址
                </label>
                <Input
                  placeholder="https://example.com"
                  value={editWebsite}
                  onChange={e => setEditWebsite(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  Logo 地址
                </label>
                <Input
                  placeholder="https://example.com/favicon.svg"
                  value={editLogo}
                  onChange={e => setEditLogo(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>
              {(editLogo || editWebsite) && (
                <div className="flex items-center gap-4">
                  <div className="w-20 shrink-0" />
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span>图标预览：</span>
                    <img
                      src={editLogo || getFaviconUrl(editWebsite) || ''}
                      alt=""
                      className="h-6 w-6 object-contain rounded"
                      onError={e => (e.currentTarget.style.display = 'none')}
                    />
                    <span className="text-muted-foreground/60">（保存后显示在渠道列表）</span>
                  </div>
                </div>
              )}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  代理地址
                </label>
                <Input
                  placeholder="http://127.0.0.1:7890 或 socks5://..."
                  value={editProxy}
                  onChange={e => setEditProxy(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!editProxy.trim() || testingProxy}
                  className="shrink-0 h-9 px-3"
                  onClick={async () => {
                    setTestingProxy(true);
                    try {
                      const res = await api.testProxy(editProxy.trim());
                      if (res.ok) {
                        toast(`延迟 ${res.latency_ms}ms｜${res.location}｜IP ${res.ip}`, 'success');
                      } else {
                        toast(res.error || '代理测试失败', 'error');
                      }
                    } catch {
                      toast('代理测试请求失败', 'error');
                    } finally {
                      setTestingProxy(false);
                    }
                  }}
                >
                  {testingProxy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                  <span className="ml-1">测试</span>
                </Button>
              </div>
            </div>
            <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-border/60">
              <Button variant="outline" onClick={() => setShowEdit(false)} className="px-5">取消</Button>
              <Button
                onClick={handleSaveEndpoint}
                disabled={saving || !editName.trim() || !editUrl.trim()}
                className="px-5 bg-blue-500 hover:bg-blue-600 text-white"
              >
                {saving ? <Loader2 className="h-4 w-4 animate-spin mr-1" /> : null}
                保存
              </Button>
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  新增端点弹窗（仿截图样式）                                          */
/* ------------------------------------------------------------------ */

function AddEndpointModal({ onAdded, onCancel }: { onAdded: () => void; onCancel: () => void }) {
  const [name, setName] = useState('');
  const [baseUrl, setBaseUrl] = useState('');
  const [websiteUrl, setWebsiteUrl] = useState('');
  const [logoUrl, setLogoUrl] = useState('');
  const [proxyUrl, setProxyUrl] = useState('');
  const [testModel, setTestModel] = useState('');
  const [adding, setAdding] = useState(false);
  const [testingProxy, setTestingProxy] = useState(false);

  async function handleSubmit() {
    if (!name.trim() || !baseUrl.trim()) return;
    setAdding(true);
    try {
      await api.addEndpoint({ name: name.trim(), base_url: baseUrl.trim(), website_url: websiteUrl.trim(), logo_url: logoUrl.trim(), proxy_url: proxyUrl.trim() });
      onAdded();
    } catch (e) {
      console.error('添加端点失败:', e);
    } finally {
      setAdding(false);
    }
  }

  return (
    <div
      className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 backdrop-blur-sm"
      onClick={onCancel}
    >
      <div
        className="w-full max-w-lg mx-4 rounded-2xl border border-border bg-card shadow-2xl"
        onClick={e => e.stopPropagation()}
      >
        {/* 标题栏 */}
        <div className="flex items-center justify-between px-6 py-4 border-b border-border/60">
          <h3 className="text-base font-semibold">创建渠道</h3>
          <button
            onClick={onCancel}
            className="flex items-center justify-center w-7 h-7 rounded-full text-muted-foreground hover:text-foreground hover:bg-muted/60 transition-colors"
          >
            <X className="h-4 w-4" />
          </button>
        </div>

        {/* 内容区 */}
        <div className="px-6 py-5 space-y-6">
          {/* 基础信息 */}
          <div>
            <h4 className="text-sm font-bold text-foreground mb-4 pb-2 border-b border-border/50">
              基础信息
            </h4>
            <div className="space-y-4">
              {/* 渠道名称 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  渠道名称 <span className="text-red-500">*</span>
                </label>
                <Input
                  placeholder="例如: OpenAI 主力"
                  value={name}
                  onChange={e => setName(e.target.value)}
                  className="flex-1 h-9"
                />
              </div>

              {/* 上游地址 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  上游地址 <span className="text-red-500">*</span>
                </label>
                <Input
                  placeholder="https://api.example.com"
                  value={baseUrl}
                  onChange={e => setBaseUrl(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>

              {/* 官网地址 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  官网地址
                </label>
                <Input
                  placeholder="https://example.com"
                  value={websiteUrl}
                  onChange={e => setWebsiteUrl(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>

              {/* Logo 地址 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  Logo 地址
                </label>
                <Input
                  placeholder="https://example.com/favicon.svg"
                  value={logoUrl}
                  onChange={e => setLogoUrl(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>
              {(logoUrl || websiteUrl) && (
                <div className="flex items-center gap-4">
                  <div className="w-20 shrink-0" />
                  <div className="flex items-center gap-2 text-xs text-muted-foreground">
                    <span>图标预览：</span>
                    <img
                      src={logoUrl || getFaviconUrl(websiteUrl) || ''}
                      alt=""
                      className="h-6 w-6 object-contain rounded"
                      onError={e => (e.currentTarget.style.display = 'none')}
                    />
                  </div>
                </div>
              )}

              {/* 代理地址 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  代理地址
                </label>
                <Input
                  placeholder="http://127.0.0.1:7890"
                  value={proxyUrl}
                  onChange={e => setProxyUrl(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
                <Button
                  variant="outline"
                  size="sm"
                  disabled={!proxyUrl.trim() || testingProxy}
                  className="shrink-0 h-9 px-3"
                  onClick={async () => {
                    setTestingProxy(true);
                    try {
                      const res = await api.testProxy(proxyUrl.trim());
                      if (res.ok) {
                        toast(`延迟 ${res.latency_ms}ms｜${res.location}｜IP ${res.ip}`, 'success');
                      } else {
                        toast(res.error || '代理测试失败', 'error');
                      }
                    } catch {
                      toast('代理测试请求失败', 'error');
                    } finally {
                      setTestingProxy(false);
                    }
                  }}
                >
                  {testingProxy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <Zap className="h-3.5 w-3.5" />}
                  <span className="ml-1">测试</span>
                </Button>
              </div>

              {/* 测试模型 */}
              <div className="flex items-center gap-4">
                <label className="w-20 shrink-0 text-sm text-muted-foreground text-right">
                  测试模型
                </label>
                <Input
                  placeholder="gpt-4.1-nano"
                  value={testModel}
                  onChange={e => setTestModel(e.target.value)}
                  className="flex-1 h-9 font-mono text-sm"
                />
              </div>
            </div>
          </div>
        </div>

        {/* 底部操作 */}
        <div className="flex items-center justify-end gap-3 px-6 py-4 border-t border-border/60">
          <Button variant="outline" onClick={onCancel} className="px-5">
            取消
          </Button>
          <Button
            onClick={handleSubmit}
            disabled={adding || !name.trim() || !baseUrl.trim()}
            className="px-5 bg-blue-500 hover:bg-blue-600 text-white"
          >
            {adding ? <Loader2 className="h-4 w-4 animate-spin mr-1" /> : null}
            创建
          </Button>
        </div>
      </div>
    </div>
  );
}

/* ------------------------------------------------------------------ */
/*  主组件（左右布局）                                                  */
/* ------------------------------------------------------------------ */

export function ChannelManager() {
  const [endpoints, setEndpoints] = useState<Endpoint[]>([]);
  const [keys, setKeys] = useState<ApiKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [showAddEndpoint, setShowAddEndpoint] = useState(false);
  const [searchEndpoint, setSearchEndpoint] = useState('');

  const refresh = useCallback(() => {
    Promise.all([api.listEndpoints(), api.listApiKeys()])
      .then(([ep, ak]) => {
        setEndpoints(ep);
        setKeys(ak);
        /* 若还没有选中，默认选第一个 */
        setSelectedId(prev => prev ?? (ep.length > 0 ? ep[0].id : null));
      })
      .catch(console.error)
      .finally(() => setLoading(false));
  }, []);

  useEffect(() => {
    setLoading(true);
    refresh();
    const timer = setInterval(refresh, 30000);
    return () => clearInterval(timer);
  }, [refresh]);

  const filteredEndpoints = endpoints.filter(ep =>
    !searchEndpoint || ep.name.toLowerCase().includes(searchEndpoint.toLowerCase())
  );

  const selectedEndpoint = endpoints.find(ep => ep.id === selectedId) ?? null;
  const selectedKeys = selectedEndpoint ? keys.filter(k => k.endpoint_id === selectedEndpoint.id) : [];

  return (
    <div className="flex h-full overflow-hidden rounded-2xl border border-border/50 bg-card shadow-sm">
      {/* ── 左侧渠道列表 ── */}
      <div className="w-56 shrink-0 flex flex-col border-r border-border/50 bg-muted/20">
        {/* 搜索框 */}
        <div className="p-3 border-b border-border/50">
          <div className="relative">
            <Search className="absolute left-2.5 top-1/2 -translate-y-1/2 h-3.5 w-3.5 text-muted-foreground" />
            <input
              value={searchEndpoint}
              onChange={e => setSearchEndpoint(e.target.value)}
              placeholder="搜索分组名称..."
              className="w-full h-8 pl-8 pr-3 text-xs rounded-lg border border-input bg-background placeholder:text-muted-foreground focus:outline-none focus:ring-2 focus:ring-primary/20 transition-all"
            />
          </div>
        </div>

        {/* 渠道列表 */}
        <div className="flex-1 overflow-y-auto p-2 space-y-0.5">
          {loading && endpoints.length === 0 ? (
            <div className="flex items-center justify-center py-8 text-muted-foreground">
              <Loader2 className="h-4 w-4 animate-spin" />
            </div>
          ) : filteredEndpoints.length === 0 ? (
            <div className="flex flex-col items-center justify-center py-8 text-muted-foreground">
              <Globe className="h-6 w-6 opacity-30 mb-2" />
              <p className="text-xs">暂无渠道</p>
            </div>
          ) : (
            filteredEndpoints.map(ep => (
              <ChannelListItem
                key={ep.id}
                endpoint={ep}
                isSelected={ep.id === selectedId}
                onClick={() => setSelectedId(ep.id)}
              />
            ))
          )}
        </div>

        {/* 底部：添加渠道按钮 */}
        <div className="p-3 border-t border-border/50">
          <Button
            size="sm"
            className="w-full h-8 text-xs gap-1.5"
            onClick={() => setShowAddEndpoint(true)}
          >
            <Plus className="h-3.5 w-3.5" /> 添加渠道
          </Button>
        </div>
      </div>

      {/* ── 右侧详情面板 ── */}
      <div className="flex-1 flex flex-col min-w-0 overflow-hidden">
        {loading && endpoints.length === 0 ? (
          <div className="flex items-center justify-center flex-1 text-muted-foreground">
            <Loader2 className="h-5 w-5 animate-spin mr-2" /> 加载中...
          </div>
        ) : selectedEndpoint ? (
          <ChannelDetailPanel
            key={selectedEndpoint.id}
            endpoint={selectedEndpoint}
            keys={selectedKeys}
            onRefresh={refresh}
          />
        ) : (
          <div className="flex flex-col items-center justify-center flex-1 text-muted-foreground">
            <Globe className="h-12 w-12 opacity-20 mb-4" />
            <p className="text-base font-medium text-foreground">选择一个渠道</p>
            <p className="text-sm mt-1">或点击「添加渠道」创建新的上游端点</p>
          </div>
        )}
      </div>

      {/* 添加渠道弹窗 */}
      {showAddEndpoint && (
        <AddEndpointModal
          onAdded={() => { setShowAddEndpoint(false); refresh(); }}
          onCancel={() => setShowAddEndpoint(false)}
        />
      )}
    </div>
  );
}
