import * as Toast from "@radix-ui/react-toast";
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

export type FeedbackTone = "success" | "info" | "warning" | "error";
export type FeedbackPlacement = "detail" | "global";
export type FeedbackInput = {
  title?: string;
  message: string;
  tone?: FeedbackTone;
  placement?: FeedbackPlacement;
  taskId?: string;
  actionLabel?: string;
  onAction?: () => void;
  retryLabel?: string;
  onRetry?: () => void;
};
type FeedbackItem = FeedbackInput & {
  id: string;
  open: boolean;
  placement: FeedbackPlacement;
  tone: FeedbackTone;
};
type FeedbackContextValue = {
  notify: (input: FeedbackInput) => string;
  dismiss: (id: string) => void;
  configure: (configuration: FeedbackConfiguration) => void;
};

export type FeedbackLabels = {
  retry: string;
  details: string;
  viewTask: string;
  dismiss: string;
  notifications: string;
};

type FeedbackConfiguration = {
  defaultPlacement?: FeedbackPlacement;
  labels: FeedbackLabels;
  onOpenTask?: (taskId: string) => void;
};

const defaultLabels: FeedbackLabels = {
  retry: "Retry",
  details: "Details",
  viewTask: "View task",
  dismiss: "Dismiss",
  notifications: "Notifications"
};

const FeedbackContext = createContext<FeedbackContextValue | null>(null);

export function FeedbackProvider({ children }: { children: ReactNode }) {
  const [items, setItems] = useState<FeedbackItem[]>([]);
  const [configuration, setConfiguration] = useState<FeedbackConfiguration>({ labels: defaultLabels });
  const configurationRef = useRef<FeedbackConfiguration>(configuration);
  const sequence = useRef(0);
  const itemsRef = useRef<FeedbackItem[]>([]);
  const removalTimers = useRef(new Map<string, number>());

  useEffect(() => {
    itemsRef.current = items;
  }, [items]);

  const dismiss = useCallback((id: string) => {
    setItems((current) => current.map((item) => item.id === id ? { ...item, open: false } : item));
    const existingTimer = removalTimers.current.get(id);
    if (existingTimer) window.clearTimeout(existingTimer);
    removalTimers.current.set(id, window.setTimeout(() => {
      removalTimers.current.delete(id);
      setItems((current) => current.filter((item) => item.id !== id));
    }, 1050));
  }, []);

  const dismissAll = useCallback(() => {
    for (const item of itemsRef.current) dismiss(item.id);
  }, [dismiss]);

  const notify = useCallback((input: FeedbackInput) => {
    const tone = input.tone ?? "info";
    const placement = input.placement ?? configurationRef.current.defaultPlacement ?? "detail";
    const dedupeKey = `${placement}:${tone}:${input.taskId ?? ""}:${input.message}`;
    let returnedId = "";
    setItems((current) => {
      const existing = current.find((item) => `${item.placement}:${item.tone}:${item.taskId ?? ""}:${item.message}` === dedupeKey);
      if (existing) {
        const existingTimer = removalTimers.current.get(existing.id);
        if (existingTimer) {
          window.clearTimeout(existingTimer);
          removalTimers.current.delete(existing.id);
        }
        returnedId = existing.id;
        return current.map((item) => item.id === existing.id ? { ...item, ...input, open: true, placement, tone } : item);
      }
      sequence.current += 1;
      returnedId = `feedback-${Date.now()}-${sequence.current}`;
      return [...current, { ...input, id: returnedId, open: true, placement, tone }];
    });
    return returnedId;
  }, []);

  const configure = useCallback((next: FeedbackConfiguration) => {
    configurationRef.current = next;
    setConfiguration(next);
  }, []);

  useEffect(() => {
    const dismissForInteraction = (event: Event) => {
      if (event.target instanceof Element && event.target.closest("[data-feedback-action]")) return;
      dismissAll();
    };
    const options = { capture: true, passive: true } as const;
    document.addEventListener("pointerdown", dismissForInteraction, options);
    document.addEventListener("keydown", dismissForInteraction, true);
    document.addEventListener("wheel", dismissForInteraction, options);
    document.addEventListener("touchstart", dismissForInteraction, options);
    document.addEventListener("scroll", dismissForInteraction, options);
    return () => {
      document.removeEventListener("pointerdown", dismissForInteraction, true);
      document.removeEventListener("keydown", dismissForInteraction, true);
      document.removeEventListener("wheel", dismissForInteraction, true);
      document.removeEventListener("touchstart", dismissForInteraction, true);
      document.removeEventListener("scroll", dismissForInteraction, true);
    };
  }, [dismissAll]);

  useEffect(() => () => {
    for (const timer of removalTimers.current.values()) window.clearTimeout(timer);
    removalTimers.current.clear();
  }, []);

  const value = useMemo(() => ({ notify, dismiss, configure }), [configure, dismiss, notify]);
  const labels = configuration.labels;
  const viewportPlacement: FeedbackPlacement = items.some((item) => item.open && item.placement === "global") ? "global" : "detail";

  return (
    <FeedbackContext.Provider value={value}>
      <Toast.Provider swipeDirection="right">
        {children}
        {items.map((item) => (
          <Toast.Root
            className="feedbackToast"
            data-placement={item.placement}
            data-tone={item.tone}
            duration={5000}
            key={item.id}
            open={item.open}
            type={item.tone === "error" ? "foreground" : "background"}
            onOpenChange={(open) => { if (!open) dismiss(item.id); }}
          >
            <div>
              {item.title ? <Toast.Title>{item.title}</Toast.Title> : null}
              <Toast.Description>{item.message}</Toast.Description>
            </div>
            {item.onRetry || item.onAction || (item.taskId && configuration.onOpenTask) ? (
              <div className="feedbackToastActions" data-radix-toast-announce-exclude="">
                {item.onRetry ? <button className="miniButton" data-feedback-action type="button" onClick={() => { item.onRetry?.(); dismissAll(); }}>{item.retryLabel ?? labels.retry}</button> : null}
                {item.onAction ? <button className="miniButton" data-feedback-action type="button" onClick={() => { item.onAction?.(); dismissAll(); }}>{item.actionLabel ?? labels.details}</button> : null}
                {!item.onAction && item.taskId && configuration.onOpenTask ? (
                  <button className="miniButton" data-feedback-action type="button" onClick={() => {
                    configuration.onOpenTask?.(item.taskId!);
                    dismissAll();
                  }}>{labels.viewTask}</button>
                ) : null}
              </div>
            ) : null}
          </Toast.Root>
        ))}
        <Toast.Viewport className="feedbackToastViewport" data-placement={viewportPlacement} aria-label={labels.notifications} />
      </Toast.Provider>
    </FeedbackContext.Provider>
  );
}

export function useFeedback() {
  const value = useContext(FeedbackContext);
  if (!value) throw new Error("useFeedback must be used inside FeedbackProvider.");
  return value;
}
