import { useState, useEffect, useCallback } from 'react';
import { Check, X, Info } from 'lucide-react';

type ToastType = 'success' | 'error' | 'info';

interface Toast {
  id: number;
  message: string;
  type: ToastType;
}

let addToastFn: ((message: string, type?: ToastType) => void) | null = null;
let nextId = 0;

/** 全局调用：显示 toast */
export function toast(message: string, type: ToastType = 'success') {
  addToastFn?.(message, type);
}

/** Toast 容器，放在 App 顶层 */
export function ToastContainer() {
  const [toasts, setToasts] = useState<Toast[]>([]);

  const addToast = useCallback((message: string, type: ToastType = 'success') => {
    const id = nextId++;
    setToasts(prev => [...prev, { id, message, type }]);
    setTimeout(() => {
      setToasts(prev => prev.filter(t => t.id !== id));
    }, 2500);
  }, []);

  useEffect(() => {
    addToastFn = addToast;
    return () => { addToastFn = null; };
  }, [addToast]);

  if (toasts.length === 0) return null;

  return (
    <div className="fixed top-4 right-4 z-[100] flex flex-col gap-2">
      {toasts.map(t => (
        <div
          key={t.id}
          className={`flex items-center gap-2 px-4 py-2.5 rounded-xl shadow-lg text-sm font-medium animate-[toast-slide-in_0.22s_ease-out] ${
            t.type === 'success'
              ? 'bg-emerald-500 text-white'
              : t.type === 'error'
              ? 'bg-red-500 text-white'
              : 'bg-card text-foreground border border-border'
          }`}
        >
          {t.type === 'success' && <Check className="h-4 w-4" />}
          {t.type === 'error' && <X className="h-4 w-4" />}
          {t.type === 'info' && <Info className="h-4 w-4" />}
          {t.message}
        </div>
      ))}
    </div>
  );
}
