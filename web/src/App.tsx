import { useState, useEffect } from 'react'
import { UsageDashboard } from '@/components/usage/UsageDashboard'
import { AdminLogin } from '@/components/AdminLogin'
import { api, setAdminSecret } from '@/lib/api'
import { useTheme } from '@/hooks/useTheme'
import { Sun, Moon } from 'lucide-react'

export default function App() {
  const [authed, setAuthed] = useState(false)
  const [checking, setChecking] = useState(true)
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
      {/* 顶部导航栏 */}
      <header className="sticky top-0 z-30 border-b border-border bg-background/80 backdrop-blur-xl">
        <div className="container mx-auto max-w-7xl px-4 sm:px-6">
          <div className="flex h-16 items-center justify-between">
            <div className="flex items-center gap-3">
              <div className="flex h-9 w-9 items-center justify-center rounded-xl bg-primary/10">
                <svg className="h-5 w-5 text-primary" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                  <path d="M12 2L2 7l10 5 10-5-10-5z" />
                  <path d="M2 17l10 5 10-5" />
                  <path d="M2 12l10 5 10-5" />
                </svg>
              </div>
              <div className="flex flex-col">
                <h1 className="text-lg font-bold leading-tight">CC-Proxy</h1>
                <span className="text-[11px] font-medium text-muted-foreground leading-none">使用统计</span>
              </div>
            </div>

            <div className="flex items-center gap-1">
              <span className="inline-flex items-center gap-1.5 rounded-full border border-emerald-500/20 bg-[hsl(var(--success-bg))] px-2.5 py-1 text-[11px] font-bold text-[hsl(var(--success))]">
                <span className="size-1.5 rounded-full bg-emerald-500" />
                在线
              </span>
              <button
                onClick={handleThemeToggle}
                className="ml-2 flex items-center justify-center size-9 rounded-xl text-muted-foreground hover:text-foreground hover:bg-accent transition-all duration-150"
                title={theme === 'dark' ? '切换浅色' : '切换深色'}
              >
                <span className={`inline-flex transition-transform duration-500 ease-out ${spinning ? 'rotate-[360deg] scale-110' : 'rotate-0 scale-100'}`}>
                  {theme === 'dark' ? <Sun className="size-[18px]" /> : <Moon className="size-[18px]" />}
                </span>
              </button>
            </div>
          </div>
        </div>
      </header>

      {/* 主内容 */}
      <div className="container mx-auto px-4 sm:px-6 py-8 max-w-7xl">
        <UsageDashboard />
      </div>
    </div>
  )
}
