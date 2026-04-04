import { useState, useEffect } from 'react'
import { ToastContainer } from '@/components/Toast'
import { UsageDashboard } from '@/components/usage/UsageDashboard'
import { RequestLogPage } from '@/components/usage/RequestLogPage'
import { ModelStatsPage } from '@/components/usage/ModelStatsPage'
import { ChannelManager } from '@/components/usage/ChannelManager'
import { AccessTokenManager } from '@/components/usage/AccessTokenManager'
import { AdminLogin } from '@/components/AdminLogin'
import { api, setAdminSecret } from '@/lib/api'
import { useTheme } from '@/hooks/useTheme'
import { Sun, Moon, LayoutDashboard, ScrollText, BarChart3, KeyRound, Network } from 'lucide-react'

type Page = 'dashboard' | 'logs' | 'models' | 'channels' | 'tokens'

const NAV_ITEMS: { key: Page; label: string; icon: React.ReactNode }[] = [
  { key: 'dashboard', label: '总览', icon: <LayoutDashboard className="size-4" /> },
  { key: 'logs', label: '请求日志', icon: <ScrollText className="size-4" /> },
  { key: 'models', label: '模型统计', icon: <BarChart3 className="size-4" /> },
  { key: 'channels', label: '渠道管理', icon: <Network className="size-4" /> },
  { key: 'tokens', label: '访问密钥', icon: <KeyRound className="size-4" /> },
]

export default function App() {
  const [authed, setAuthed] = useState(false)
  const [checking, setChecking] = useState(true)
  const [page, setPage] = useState<Page>('dashboard')
  const { theme, toggle } = useTheme()
  const [spinning, setSpinning] = useState(false)

  const handleThemeToggle = (e: React.MouseEvent) => {
    setSpinning(true)
    toggle(e)
    setTimeout(() => setSpinning(false), 500)
  }

  useEffect(() => {
    const saved = sessionStorage.getItem('admin_secret')
    if (saved) {
      api.verifyAdmin(saved).then(r => {
        if (r.valid) {
          setAdminSecret(saved)
          setAuthed(true)
        }
        setChecking(false)
      }).catch(() => setChecking(false))
    } else {
      api.verifyAdmin('').then(r => {
        if (!r.auth_required) {
          setAuthed(true)
        }
        setChecking(false)
      }).catch(() => setChecking(false))
    }
  }, [])

  if (checking) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="text-center">
          <div className="spinner mx-auto mb-3" />
          <p className="text-sm text-muted-foreground">加载中...</p>
        </div>
      </div>
    )
  }

  if (!authed) {
    return <AdminLogin onSuccess={() => setAuthed(true)} theme={theme} onThemeToggle={handleThemeToggle} />
  }

  return (
    <div className="min-h-screen bg-background">
      <ToastContainer />
      <header className="sticky top-0 z-30 border-b border-border bg-background/80 backdrop-blur-xl">
        <div className="container mx-auto max-w-7xl px-4 sm:px-6">
          <div className="flex h-14 items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-8 w-8 items-center justify-center rounded-lg overflow-hidden">
                <img src="/favicon.ico" alt="logo" className="h-7 w-7" />
              </div>
              <h1 className="text-base font-bold">completions-to-messages</h1>
            </div>

            <div className="flex items-center gap-2">
              <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-[hsl(var(--success-bg))] px-2.5 py-1 text-[11px] font-bold text-[hsl(var(--success))]">
                <span className="size-1.5 rounded-full bg-emerald-500" />
                在线
              </span>
              <button
                onClick={handleThemeToggle}
                className="flex items-center justify-center size-8 rounded-lg text-muted-foreground hover:text-foreground hover:bg-accent transition-all duration-150"
                title={theme === 'dark' ? '切换浅色' : '切换深色'}
              >
                <span className={`inline-flex transition-transform duration-500 ease-out ${spinning ? 'rotate-[360deg] scale-110' : 'rotate-0 scale-100'}`}>
                  {theme === 'dark' ? <Sun className="size-[16px]" /> : <Moon className="size-[16px]" />}
                </span>
              </button>
            </div>
          </div>

          <nav className="flex gap-1 -mb-px overflow-x-auto">
            {NAV_ITEMS.map((item) => (
              <button
                key={item.key}
                onClick={() => setPage(item.key)}
                className={`inline-flex items-center gap-2 px-4 py-2.5 text-sm font-medium border-b-2 transition-colors whitespace-nowrap ${
                  page === item.key
                    ? 'border-primary text-primary'
                    : 'border-transparent text-muted-foreground hover:text-foreground hover:border-border'
                }`}
              >
                {item.icon}
                {item.label}
              </button>
            ))}
          </nav>
        </div>
      </header>

      <div className={`container mx-auto px-4 sm:px-6 max-w-7xl ${page === 'channels' ? 'py-4' : 'py-8'}`}>
        {page === 'dashboard' && <UsageDashboard />}
        {page === 'logs' && <RequestLogPage />}
        {page === 'models' && <ModelStatsPage />}
        {page === 'channels' && (
          <div style={{ height: 'calc(100vh - 120px)' }}>
            <ChannelManager />
          </div>
        )}
        {page === 'tokens' && <AccessTokenManager />}
      </div>
    </div>
  )
}
