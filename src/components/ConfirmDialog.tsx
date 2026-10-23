"use client";
import { useId, useRef, useState } from "react";
import { LoaderCircle } from "lucide-react";
import { useModalFocus } from "@/hooks/useModalFocus";
export function ConfirmDialog({ title, message, detail, confirmLabel = "确认", cancelLabel = "取消", danger = false, confirmDisabled = false, onConfirm, onCancel }: {
  title: string; message: string; detail?: string; confirmLabel?: string; cancelLabel?: string;
  danger?: boolean; confirmDisabled?: boolean; onConfirm: () => void | Promise<void>; onCancel: () => void;
}) {
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const running = useRef(false);
  const id = useId();
  const ref = useModalFocus(true, () => { if (!running.current) onCancel(); });
  const confirm = async () => {
    if (running.current || confirmDisabled) return;
    running.current = true; setPending(true); setError(null);
    try { await onConfirm(); onCancel(); }
    catch (cause) { setError(String(cause)); }
    finally { running.current = false; setPending(false); }
  };
  return <div className="fixed inset-0 z-[70] flex items-center justify-center p-4" style={{ background: "rgba(0,0,0,.35)" }}>
    <div ref={ref} role="dialog" aria-modal="true" aria-labelledby={id} aria-describedby={`${id}-message`} tabIndex={-1}
      style={{ width: 420, maxWidth: "100%", padding: 24, borderRadius: 22, background: "var(--card-bg)", color: "var(--foreground)" }}>
      <h3 id={id}>{title}</h3><p id={`${id}-message`}>{message}</p>{detail && <p>{detail}</p>}
      {error && <p role="alert" style={{ color: "var(--danger)" }}>{error}</p>}
      <div className="mt-5 flex justify-end gap-3">
        <button data-autofocus disabled={pending} onClick={onCancel}>{cancelLabel}</button>
        <button disabled={pending || confirmDisabled} onClick={() => void confirm()} style={{ color: danger ? "var(--danger)" : "var(--accent)" }}>
          {pending && <LoaderCircle size={14} className="inline animate-spin" aria-hidden="true" />}
          {pending ? "处理中…" : confirmLabel}
        </button>
      </div>
    </div>
  </div>;
}
