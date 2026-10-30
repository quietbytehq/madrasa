import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";
import { AlertTriangle, CheckCircle2, Info, X, XCircle } from "lucide-react";
import { cn } from "@/lib/cn";
import { useI18n } from "@/i18n";

export type ToastKind = "info" | "success" | "warning" | "error";

export interface ToastInput {
  title: ReactNode;
  description?: ReactNode;
  kind?: ToastKind;
  /** Milliseconds before auto-dismiss; 0 keeps it until closed. Default 5000. */
  duration?: number;
}

export interface Toast extends ToastInput {
  id: number;
  kind: ToastKind;
}

interface ToastContextValue {
  toasts: Toast[];
  push: (toast: ToastInput) => number;
  dismiss: (id: number) => void;
}

const ToastContext = createContext<ToastContextValue | null>(null);

export function ToastProvider({ children }: { children: ReactNode }) {
  const [toasts, setToasts] = useState<Toast[]>([]);
  const nextId = useRef(1);
  const timers = useRef(new Map<number, ReturnType<typeof setTimeout>>());

  const dismiss = useCallback((id: number) => {
    const timer = timers.current.get(id);
    if (timer) clearTimeout(timer);
    timers.current.delete(id);
    setToasts((list) => list.filter((t) => t.id !== id));
  }, []);

  const push = useCallback(
    (input: ToastInput) => {
      const id = nextId.current++;
      const toast: Toast = { kind: "info", ...input, id };
      setToasts((list) => [...list, toast]);
      const duration = input.duration ?? 5000;
      if (duration > 0)
        timers.current.set(
          id,
          setTimeout(() => dismiss(id), duration),
        );
      return id;
    },
    [dismiss],
  );

  const value = useMemo(() => ({ toasts, push, dismiss }), [toasts, push, dismiss]);

  return (
    <ToastContext.Provider value={value}>
      {children}
      <ToastViewport />
    </ToastContext.Provider>
  );
}

export function useToast(): Pick<ToastContextValue, "push" | "dismiss"> {
  const ctx = useContext(ToastContext);
  if (!ctx) throw new Error("useToast must be used inside <ToastProvider>");
  return { push: ctx.push, dismiss: ctx.dismiss };
}

const kindStyle: Record<ToastKind, { icon: typeof Info; className: string }> = {
  info: { icon: Info, className: "text-blue" },
  success: { icon: CheckCircle2, className: "text-success" },
  warning: { icon: AlertTriangle, className: "text-warning" },
  error: { icon: XCircle, className: "text-danger" },
};

function ToastViewport() {
  const ctx = useContext(ToastContext);
  const { t } = useI18n();
  if (!ctx) return null;
  return (
    <div
      aria-live="polite"
      aria-relevant="additions"
      className="pointer-events-none fixed bottom-4 end-4 z-[60] flex w-80 max-w-[calc(100vw-2rem)] flex-col gap-2"
    >
      {ctx.toasts.map((toast) => {
        const { icon: Icon, className } = kindStyle[toast.kind];
        return (
          <div
            key={toast.id}
            role={toast.kind === "error" ? "alert" : "status"}
            className="pointer-events-auto flex items-start gap-2 rounded-lg border border-border bg-card p-3 shadow-pop"
          >
            <Icon className={cn("mt-0.5 h-4 w-4 shrink-0", className)} aria-hidden />
            <div className="min-w-0 flex-1">
              <p className="text-sm font-medium text-text">{toast.title}</p>
              {toast.description && (
                <p className="mt-0.5 text-xs text-muted">{toast.description}</p>
              )}
            </div>
            <button
              type="button"
              onClick={() => ctx.dismiss(toast.id)}
              aria-label={t("toast.close")}
              className="rounded p-0.5 text-muted hover:bg-hover hover:text-text"
            >
              <X className="h-3.5 w-3.5" aria-hidden />
            </button>
          </div>
        );
      })}
    </div>
  );
}
