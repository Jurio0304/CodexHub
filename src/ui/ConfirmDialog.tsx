import { AlertModalFrame } from "./AlertModalFrame";

export type ConfirmDialogCopy = {
  title: string;
  body: string;
  cancel: string;
  confirm: string;
};

export function ConfirmDialog({
  copy,
  open,
  onCancel,
  onConfirm
}: {
  copy: ConfirmDialogCopy;
  open: boolean;
  onCancel: () => void;
  onConfirm: () => void;
}) {
  if (!open) return null;
  return (
    <AlertModalFrame
      className="taskLogModal simpleDeleteModal confirmDialog"
      descriptionId="confirm-dialog-description"
      titleId="confirm-dialog-title"
      onCancel={onCancel}
    >
      <div className="modalHeader taskLogModalHeader">
        <div className="modalTitleBlock">
          <div>
            <h2 id="confirm-dialog-title">{copy.title}</h2>
            <p id="confirm-dialog-description">{copy.body}</p>
          </div>
        </div>
      </div>
      <div className="modalActions">
        <button className="secondaryButton" data-alert-cancel type="button" onClick={onCancel}>{copy.cancel}</button>
        <button className="primaryButton dangerButton" type="button" onClick={onConfirm}>{copy.confirm}</button>
      </div>
    </AlertModalFrame>
  );
}
