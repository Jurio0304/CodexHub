import * as AlertDialog from "@radix-ui/react-alert-dialog";
import { useRef } from "react";
import type { ReactNode } from "react";

export function AlertModalFrame({
  busy = false,
  children,
  className = "",
  descriptionId,
  onCancel,
  titleId
}: {
  busy?: boolean;
  children: ReactNode;
  className?: string;
  descriptionId?: string;
  onCancel: () => void;
  titleId: string;
}) {
  const returnFocusRef = useRef<HTMLElement | null>(
    typeof document !== "undefined" && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null
  );
  const contentRef = useRef<HTMLDivElement | null>(null);

  return (
    <AlertDialog.Root
      open
      onOpenChange={(nextOpen) => {
        if (!nextOpen && !busy) onCancel();
      }}
    >
      <AlertDialog.Portal>
        <AlertDialog.Overlay className="modalBackdrop" />
        <AlertDialog.Content
          aria-describedby={descriptionId}
          aria-labelledby={titleId}
          className={`modalFrame portalModalContent ${className}`.trim()}
          ref={contentRef}
          onCloseAutoFocus={(event) => {
            event.preventDefault();
            const returnTarget = returnFocusRef.current;
            if (returnTarget?.isConnected) returnTarget.focus();
            else document.querySelector<HTMLElement>('.navItem[data-active="true"]')?.focus();
          }}
          onEscapeKeyDown={(event) => {
            if (busy) event.preventDefault();
          }}
          onOpenAutoFocus={(event) => {
            const cancel = contentRef.current?.querySelector<HTMLElement>("[data-alert-cancel]:not(:disabled)");
            if (cancel) {
              event.preventDefault();
              cancel.focus();
            }
          }}
        >
          {children}
        </AlertDialog.Content>
      </AlertDialog.Portal>
    </AlertDialog.Root>
  );
}
