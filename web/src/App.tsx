import { useState, useEffect } from 'react'
import { UsageDashboard } from '@/components/usage/UsageDashboard'
import { AdminLogin } from '@/components/AdminLogin'
import { api, setAdminSecret } from '@/lib/api'

export default function App() {
  const [authed, setAuthed] = useState(false)
  const [checking, setChecking] = useState(true)

  useEffect(() => {
    // 检查是否需要鉴权 + 已有的 session 是否有效
    const saved = sessionStorage.getItem('admin_secret')
    if (saved) {
      // 用已保存的密钥尝试验证
      api.verifyAdmin(saved).then(r => {
        if (r.valid) {
          setAdminSecret(saved)
          setAuthed(true)
        }
        setChecking(false)
      }).catch(() => setChecking(false))
    } else {
      // 检查是否需要鉴权
      api.verifyAdmin('').then(r => {
        if (!r.auth_required) {
          // 未设置 ADMIN_SECRET，直接放行
          setAuthed(true)
        }
        setChecking(false)
      }).catch(() => setChecking(false))
    }
  }, [])

  if (checking) {
    return (
      <div className="min-h-screen bg-background flex items-center justify-center">
        <div className="animate-pulse text-muted-foreground">加载中...</div>
      </div>
    )
  }

  if (!authed) {
    return <AdminLogin onSuccess={() => setAuthed(true)} />
  }

  return (
    <div className="min-h-screen bg-background">
      <div className="container mx-auto px-4 py-8 max-w-7xl">
        <UsageDashboard />
      </div>
    </div>
  )
}
