import { useState } from 'react';
import { Card, CardContent } from '@/components/ui/card';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { api, setAdminSecret } from '@/lib/api';
import { Shield, LogIn, Loader2 } from 'lucide-react';

interface LoginProps {
  onSuccess: () => void;
}

export function AdminLogin({ onSuccess }: LoginProps) {
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
    <div className="min-h-screen bg-background flex items-center justify-center p-4">
      <Card className="w-full max-w-sm border border-border/50 bg-card/80 backdrop-blur-sm shadow-lg">
        <CardContent className="p-6 space-y-4">
          <div className="flex flex-col items-center gap-2 mb-2">
            <div className="p-3 rounded-full bg-primary/10">
              <Shield className="h-6 w-6 text-primary" />
            </div>
            <h1 className="text-lg font-semibold">CompletionsToMessages</h1>
            <p className="text-sm text-muted-foreground">请输入管理密钥</p>
          </div>

          <Input
            type="password"
            placeholder="ADMIN_SECRET"
            value={secret}
            onChange={e => setSecret(e.target.value)}
            onKeyDown={handleKeyDown}
            autoFocus
          />

          {error && (
            <p className="text-sm text-red-500 text-center">{error}</p>
          )}

          <Button className="w-full gap-2" onClick={handleLogin} disabled={!secret.trim() || loading}>
            {loading ? <Loader2 className="h-4 w-4 animate-spin" /> : <LogIn className="h-4 w-4" />}
            登录
          </Button>
        </CardContent>
      </Card>
    </div>
  );
}
