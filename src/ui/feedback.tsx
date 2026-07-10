import * as Toast from "@radix-ui/react-toast";
import { createContext, useCallback, useContext, useMemo, useRef, useState } from "react";
import type { ReactNode } from "react";

export type FeedbackTone = "success" | "info" | "warning" | "error";
export type FeedbackInput = {
  title?: string;
  message: string;
  tone?: FeedbackTone;
  taskId?: string;
  actionLabel?: string;
  onAction?: () => void;
  retryLabel?: string;
  onRetry?: () => void;
};
type FeedbackItem = FeedbackInput & {
  id: string;
  tone: FeedbackTone;
};
type FeedbackContextValue = {
  notify: (input: FeedbackInput) => string;
  dismiss: (id: string) => void;
  configure: (configuration: FeedbackConfiguration) => void;
};

export type FeedbackLabels = {
  persistentRegion: string;
  retry: string;
  details: string;
  viewTask: string;
  dismiss: string;
  notifications: string;
};

type FeedbackConfiguration = {
  labels: FeedbackLabels;
  onOpenTask?: (taskId: string) => void;
};

const defaultLabels: FeedbackLabels = {
  persistentRegion: "Persistent errors",
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
  const sequence = useRef(0);

  const dismiss = useCallback((id: string) => {
    setItems((current) => current.filter((item) => item.id !== id));
  }, []);

  const notify = useCallback((input: FeedbackInput) => {
    const tone = input.tone ?? "info";
    const dedupeKey = `${tone}:${input.taskId ?? ""}:${input.message}`;
    let returnedId = "";
    setItems((current) => {
      const existing = current.find((item) => `${item.tone}:${item.taskId ?? ""}:${item.message}` === dedupeKey);
      if (existing) {
        returnedId = existing.id;
        return current.map((item) => item.id === existing.id ? { ...item, ...input, tone } : item);
      }
      sequence.current += 1;
      returnedId = `feedback-${Date.now()}-${sequence.current}`;
      return [...current, { ...input, id: returnedId, tone }];
    });
    return returnedId;
  }, []);

  const configure = useCallback((next: FeedbackConfiguration) => {
    setConfiguration(next);
  }, []);

  const value = useMemo(() => ({ notify, dismiss, configure }), [configure, dismiss, notify]);
  const persistent = items.filter((item) => item.tone === "error");
  const transient = items.filter((item) => item.tone !== "error");
  const labels = configuration.labels;

  return (
    <FeedbackContext.Provider value={value}>
      <Toast.Provider swipeDirection="right">
        {children}
        {persistent.length > 0 ? (
          <aside className="persistentFeedbackRegion" aria-label={labels.persistentRegion}>
            {persistent.map((item) => (
              <section className="persistentFeedback" data-tone={item.tone} key={item.id} role="alert">
                <div>
                  {item.title ? <strong>{item.title}</strong> : null}
                  <span>{item.message}</span>
                </div>
                <div className="persistentFeedbackActions">
                  {item.onRetry ? <button className="miniButton" type="button" onClick={item.onRetry}>{item.retryLabel ?? labels.retry}</button> : null}
                  {item.onAction ? <button className="miniButton" type="button" onClick={item.onAction}>{item.actionLabel ?? labels.details}</button> : null}
                  {!item.onAction && item.taskId && configuration.onOpenTask ? (
                    <button className="miniButton" type="button" onClick={() => {
                      configuration.onOpenTask?.(item.taskId!);
                      dismiss(item.id);
                    }}>{labels.viewTask}</button>
                  ) : null}
                  <button className="modalCloseButton" type="button" aria-label={labels.dismiss} onClick={() => dismiss(item.id)}>×</button>
                </div>
              </section>
            ))}
          </aside>
        ) : null}
        {transient.map((item) => (
          <Toast.Root
            className="feedbackToast"
            data-tone={item.tone}
            duration={item.tone === "warning" ? 8000 : 5000}
            key={item.id}
            onOpenChange={(open) => { if (!open) dismiss(item.id); }}
          >
            {item.title ? <Toast.Title>{item.title}</Toast.Title> : null}
            <Toast.Description>{item.message}</Toast.Description>
            {item.onAction ? <Toast.Action altText={item.actionLabel ?? labels.details} asChild><button className="miniButton" type="button" onClick={item.onAction}>{item.actionLabel ?? labels.details}</button></Toast.Action> : null}
            <Toast.Close className="modalCloseButton" aria-label={labels.dismiss}>×</Toast.Close>
          </Toast.Root>
        ))}
        <Toast.Viewport className="feedbackToastViewport" aria-label={labels.notifications} />
      </Toast.Provider>
    </FeedbackContext.Provider>
  );
}

export function useFeedback() {
  const value = useContext(FeedbackContext);
  if (!value) throw new Error("useFeedback must be used inside FeedbackProvider.");
  return value;
}
