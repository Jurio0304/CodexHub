import * as Dialog from "@radix-ui/react-dialog";
import { useRef } from "react";
import type { ReactNode } from "react";

export function ModalFrame({
  children,
  className = "",
  titleId
}: {
  children: ReactNode;
  className?: string;
  titleId: string;
}) {
  const returnFocusRef = useRef<HTMLElement | null>(
    typeof document !== "undefined" && document.activeElement instanceof HTMLElement
      ? document.activeElement
      : null
  );
  const closeThroughExistingPath = (content: HTMLElement) => {
    content.querySelector<HTMLButtonElement>(".modalCloseButton:not(:disabled)")?.click();
  };

  return (
    <Dialog.Root
      open
      onOpenChange={(nextOpen) => {
        if (nextOpen) return;
        const content = document.querySelector<HTMLElement>(`[data-modal-title-id="${titleId}"]`);
        if (content) closeThroughExistingPath(content);
      }}
    >
      <Dialog.Content
        aria-labelledby={titleId}
        className={`modalFrame ${className}`.trim()}
        data-modal-title-id={titleId}
        onCloseAutoFocus={(event) => {
          event.preventDefault();
          const returnTarget = returnFocusRef.current;
          if (returnTarget?.isConnected) returnTarget.focus();
          else document.querySelector<HTMLElement>('.navItem[data-active="true"]')?.focus();
        }}
        onEscapeKeyDown={(event) => {
          const content = event.currentTarget as HTMLElement;
          if (!content.querySelector(".modalCloseButton:not(:disabled)")) event.preventDefault();
        }}
        onOpenAutoFocus={(event) => {
          const content = event.currentTarget as HTMLElement;
          const cancel = content.querySelector<HTMLElement>(".dangerButton")
            ? content.querySelector<HTMLElement>(".modalActions .secondaryButton:not(:disabled)")
            : null;
          const editable = content.querySelector<HTMLElement>(
            "input:not(:disabled):not([readonly]), textarea:not(:disabled):not([readonly]), select:not(:disabled)"
          );
          const first = cancel ?? editable ?? content.querySelector<HTMLElement>(
            "button:not(:disabled), [href], [tabindex]:not([tabindex='-1'])"
          );
          if (first) {
            event.preventDefault();
            first.focus();
          }
        }}
        onPointerDownOutside={(event) => event.preventDefault()}
      >
        {children}
      </Dialog.Content>
    </Dialog.Root>
  );
}
