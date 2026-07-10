import * as Dialog from "@radix-ui/react-dialog";
import type { TaskRun } from "../generated/rust-contracts";

export type TaskDrawerCopy = {
  open: string;
  title: string;
  description: string;
  close: string;
  viewAll: string;
  empty: string;
  details: string;
  attentionCount: (count: number) => string;
};

export function TaskDrawer({
  copy,
  tasks,
  unacknowledgedTaskIds,
  statusLabel,
  onOpenTask,
  onViewAll
}: {
  copy: TaskDrawerCopy;
  tasks: TaskRun[];
  unacknowledgedTaskIds: ReadonlySet<string>;
  statusLabel: (task: TaskRun) => string;
  onOpenTask: (taskId: string) => void;
  onViewAll: () => void;
}) {
  const priority = tasks.filter((task) => task.status === "running" || unacknowledgedTaskIds.has(task.id));
  const recent = [...priority, ...tasks.filter((task) => !priority.some((item) => item.id === task.id))].slice(0, 20);
  const attentionCount = priority.length;

  return (
    <Dialog.Root>
      <Dialog.Trigger asChild>
        <button className="taskDrawerTrigger" type="button" aria-label={copy.open}>
          <span aria-hidden="true">✓</span>
          <span>{copy.open}</span>
          {attentionCount > 0 ? <strong aria-label={copy.attentionCount(attentionCount)}>{attentionCount}</strong> : null}
        </button>
      </Dialog.Trigger>
      <Dialog.Portal>
        <Dialog.Overlay className="taskDrawerOverlay" />
        <Dialog.Content
          className="taskDrawer"
          onPointerDownOutside={(event) => event.preventDefault()}
        >
          <header className="taskDrawerHeader">
            <div>
              <Dialog.Title>{copy.title}</Dialog.Title>
              <Dialog.Description>{copy.description}</Dialog.Description>
            </div>
            <Dialog.Close className="modalCloseButton" aria-label={copy.close}>×</Dialog.Close>
          </header>
          <div className="taskDrawerList">
            {recent.length === 0 ? <p className="taskDrawerEmpty">{copy.empty}</p> : recent.map((task) => (
              <Dialog.Close asChild key={task.id}>
                <button
                  className="taskDrawerItem"
                  data-attention={unacknowledgedTaskIds.has(task.id)}
                  type="button"
                  onClick={() => onOpenTask(task.id)}
                >
                  <span>
                    <strong>{task.action}</strong>
                    <small>{task.hostName}</small>
                  </span>
                  <span>
                    <small>{statusLabel(task)}</small>
                    <i>{copy.details}</i>
                  </span>
                </button>
              </Dialog.Close>
            ))}
          </div>
          <footer className="taskDrawerFooter">
            <Dialog.Close asChild>
              <button className="secondaryButton" type="button" onClick={onViewAll}>{copy.viewAll}</button>
            </Dialog.Close>
          </footer>
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}
