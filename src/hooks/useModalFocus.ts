"use client";
import { useEffect, useRef } from "react";
const stack: symbol[] = [];
export function useModalFocus(open: boolean, onClose: () => void) {
  const ref = useRef<HTMLDivElement>(null);
  const close = useRef(onClose);
  close.current = onClose;
  useEffect(() => {
    if (!open) return;
    const panel = ref.current;
    if (!panel) return;
    const previous = document.activeElement as HTMLElement | null;
    const id = Symbol(); stack.push(id);
    const focusables = () => [...panel.querySelectorAll<HTMLElement>(
      'button:not(:disabled),input:not(:disabled),select:not(:disabled),textarea:not(:disabled),a[href],[tabindex="0"]'
    )].filter(el => el.getClientRects().length > 0);
    (panel.querySelector<HTMLElement>('[data-autofocus]') ?? focusables()[0] ?? panel).focus();
    const keydown = (event: KeyboardEvent) => {
      if (stack.at(-1) !== id || event.isComposing) return;
      if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); close.current(); }
      if (event.key === "Tab") {
        const list = focusables();
        const first = list[0] ?? panel, last = list.at(-1) ?? panel;
        if (event.shiftKey && (document.activeElement === first || !panel.contains(document.activeElement))) {
          event.preventDefault(); last.focus();
        } else if (!event.shiftKey && (document.activeElement === last || !panel.contains(document.activeElement))) {
          event.preventDefault(); first.focus();
        }
      }
    };
    document.addEventListener("keydown", keydown, true);
    return () => {
      stack.splice(stack.indexOf(id), 1);
      document.removeEventListener("keydown", keydown, true);
      if (previous?.isConnected) previous.focus();
      else document.querySelector<HTMLElement>('main button, main a[href]')?.focus();
    };
  }, [open]);
  return ref;
}
