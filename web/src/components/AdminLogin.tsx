import { useState } from 'react';
import { api, setAdminSecret } from '@/lib/api';
import { Shield, LogIn, Loader2, Sun, Moon } from 'lucide-react';

interface LoginProps {
  onSuccess: () => void;
  theme: 'light' | 'dark';
  onThemeToggle: (e: React.MouseEvent) => void;
}

export function AdminLogin({ onSuccess, theme, onThemeToggle }: LoginProps) {
  const [secret, setSecret] = useState('');
  const [error, setError] = useState('');
  const [loading, setLoading] = useState(false);

  async function handleLogin() {
    if (!secret.trim()) return;
    setLoading(true);
    setError('');
    try {
      const result = await api.verifyAdmin(secret.trim());
      if (result.valid) {
        setAdminSecret(secret.trim());
        onSuccess();
      } else {
        setError('密钥无效');
      }
    } catch {
      setError('连接失败');
    } finally {
      setLoading(false);
    }
  }

  function handleKeyDown(e: React.KeyboardEvent) {
    if (e.key === 'Enter') handleLogin();
  }

  return (
    <div className="min-h-screen flex items-center justify-center p-4 bg-gradient-to-br from-slate-50 via-white to-blue-50/30 dark:from-[hsl(222,84%,4.9%)] dark:via-[hsl(222,47%,6%)] dark:to-[hsl(222,47%,8%)]">
      {/* 右上角主题切换 */}
      <button
        onClick={onThemeToggle}
        className="fixed top-5 right-5 flex items-center justify-center size-9 rounded-xl text-muted-foreground hover:text-foreground hover:bg-white/60 dark:hover:bg-white/10 transition-all duration-150"
        title={theme === 'dark' ? '切换浅色' : '切换深色'}
      >
        {theme === 'dark' ? <Sun className="size-[18px]" /> : <Moon className="size-[18px]" />}
      </button>

      <div className="w-full max-w-[400px]">
        {/* Logo + 标题 */}
        <div className="text-center mb-8">
          <div className="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-2xl bg-primary/10 shadow-lg shadow-primary/10">
            <Shield className="h-8 w-8 text-primary" />
          </div>
          <h1 className="text-[28px] font-bold bg-gradient-to-br from-primary to-blue-400 bg-clip-text text-transparent">
            completions-to-messages
          </h1>
          <p className="text-sm text-muted-foreground mt-1">请输入管理密钥以继续</p>
        </div>

        {/* 登录卡片 */}
        <div className="rounded-3xl border border-border bg-card/80 shadow-xl shadow-black/[0.03] p-6 backdrop-blur-sm">
          <div className="space-y-4">
            <div>
              <label className="block mb-2 text-sm font-semibold text-muted-foreground">管理密钥</label>
              <input
                type="password"
                placeholder="ADMIN_SECRET"
                value={secret}
                onChange={e => { setSecret(e.target.value); setError(''); }}
                onKeyDown={handleKeyDown}
                autoFocus
                className="w-full h-11 px-4 rounded-xl border border-border bg-background text-[15px] outline-none transition-all focus:border-primary/40 focus:ring-2 focus:ring-primary/10"
              />
            </div>

            {error && (
              <div className="text-sm text-red-500 font-medium px-1">{error}</div>
            )}

            <button
              className="w-full h-11 rounded-xl bg-gradient-to-r from-primary to-blue-400 text-primary-foreground font-semibold text-[15px] shadow-lg shadow-primary/20 transition-all hover:opacity-90 disabled:opacity-50 flex items-center justify-center gap-2"
              onClick={handleLogin}
              disabled={!secret.trim() || loading}
            >
              {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <LogIn className="h-4 w-4" />}
              登录
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}
