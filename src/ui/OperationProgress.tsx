import { useEffect, useRef, useState } from "react";
import type { KeyboardEvent, ReactNode } from "react";
import type { HostOperationKind, TaskLog, TaskRun, TaskStep, TaskStepStatus } from "../models";
import { ModalFrame } from "./ModalFrame";

export type OperationOverallStatus = "running" | "success" | "failed" | "partial";

export type OperationProgressHost = {
  hostAlias: string;
  hostName: string;
  status: OperationOverallStatus | "pending";
  steps: TaskStep[];
  logs: TaskLog[];
  message?: string;
  finalized?: boolean;
};

export type OperationProgressCopy = {
  close: string;
  hide: string;
  viewTasks: string;
  hostSelector: string;
  progress: string;
  details: string;
  noLogs: string;
  noOutput: string;
  command: string;
  exitCode: string;
  duration: string;
  timedOut: string;
  stdout: string;
  stderr: string;
  yes: string;
  no: string;
  status: Record<TaskStepStatus, string>;
  overallStatus: Record<OperationOverallStatus, string>;
};

export type OperationStepPresentation = {
  title: string;
  summary: string;
};

export function operationHostStatusFromSteps(
  operation: HostOperationKind,
  steps: TaskStep[]
): OperationProgressHost["status"] {
  const terminal = (status: TaskStepStatus) => status === "success" || status === "failed" || status === "skipped";
  if (operation === "host-test") {
    if (steps.length === 0 || steps.some((step) => !terminal(step.status))) return "running";
    return steps.some((step) => step.status === "failed") ? "failed" : "success";
  }

  const finalVerification = steps.find((step) => step.stepId === "final-verification");
  if (finalVerification?.status === "success") return "success";
  if (finalVerification?.status === "failed") return "failed";
  const preparation = steps.find((step) => step.stepId === "preparation");
  if (preparation?.status === "failed" && steps.every((step) => terminal(step.status))) return "failed";
  if (finalVerification?.status === "skipped" && steps.every((step) => terminal(step.status))) return "failed";
  return "running";
}

export function mergeOperationProgressHost(
  host: OperationProgressHost,
  operation: HostOperationKind,
  incomingStep: TaskStep,
  incomingLog?: TaskLog
): OperationProgressHost {
  // 最终 TaskRun 是权威快照，迟到的 IPC 事件不得再修改它。
  if (host.finalized) return host;
  const currentStep = host.steps.find((step) => step.stepId === incomingStep.stepId);
  const incomingIsPending = incomingStep.status === "pending" || incomingStep.status === "running";
  const currentIsTerminal = currentStep ? isTerminalStepStatus(currentStep.status) : false;
  const hostIsTerminal = host.status === "success" || host.status === "failed" || host.status === "partial";
  if (!currentStep && hostIsTerminal && incomingIsPending) return host;

  const mergedStep = currentStep && currentIsTerminal && incomingIsPending ? currentStep : incomingStep;
  const stepIndex = host.steps.findIndex((step) => step.stepId === incomingStep.stepId);
  const steps = stepIndex >= 0
    ? host.steps.map((step, index) => (index === stepIndex ? mergedStep : step))
    : [...host.steps, mergedStep].sort((left, right) => left.sequence - right.sequence);
  const logs = incomingLog && !host.logs.some((log) => log.id === incomingLog.id)
    ? [...host.logs, incomingLog]
    : host.logs;
  const derivedStatus = operationHostStatusFromSteps(operation, steps);
  const status = hostIsTerminal && (derivedStatus === "running" || derivedStatus === "pending")
    ? host.status
    : derivedStatus;
  return { ...host, status, steps, logs };
}

export function finalizeOperationProgressHost(
  host: OperationProgressHost,
  task: TaskRun,
  ok: boolean,
  message: string
): OperationProgressHost {
  return {
    ...host,
    status: ok ? "success" : "failed",
    steps: (task.steps ?? []).length > 0 ? task.steps : host.steps,
    logs: task.logs,
    message,
    finalized: true
  };
}

export function settleOperationStepsAfterFailure(
  steps: TaskStep[],
  message: string,
  endedAt = new Date().toISOString()
) {
  return steps.map((step) => {
    if (step.status === "running") return { ...step, status: "failed" as const, summary: message, endedAt };
    if (step.status === "pending") return { ...step, status: "skipped" as const, summary: message, endedAt };
    return step;
  });
}

function isTerminalStepStatus(status: TaskStepStatus) {
  return status === "success" || status === "failed" || status === "skipped";
}

export function OperationStatusIcon({
  status
}: {
  status: TaskStepStatus;
}) {
  return (
    <span aria-hidden="true" className="operationStatusIcon" data-status={status}>
      {status === "running" ? <i className="operationStatusSpinner" aria-hidden="true" /> : null}
      {status === "pending" || status === "skipped" ? (
        <span className="operationStatusDots" aria-hidden="true"><i /><i /><i /></span>
      ) : null}
      {status === "success" ? <span aria-hidden="true">✓</span> : null}
      {status === "failed" ? <span aria-hidden="true">×</span> : null}
    </span>
  );
}

export function OperationStepCard({
  copy,
  logs,
  presentation,
  step
}: {
  copy: OperationProgressCopy;
  logs: TaskLog[];
  presentation: OperationStepPresentation;
  step: TaskStep;
}) {
  const [expanded, setExpanded] = useState(false);
  const contentId = `operation-step-${safeDomId(step.taskRunId)}-${safeDomId(step.stepId)}`;

  return (
    <section className="operationStepCard" data-status={step.status}>
      <button
        aria-controls={contentId}
        aria-expanded={expanded}
        aria-label={`${presentation.title}. ${presentation.summary}. ${copy.status[step.status]}`}
        className="operationStepSummary"
        type="button"
        onClick={() => setExpanded((current) => !current)}
      >
        <span className="operationStepText">
          <strong>{presentation.title}</strong>
          <small>{presentation.summary}</small>
        </span>
        <span className="operationStepState">{copy.status[step.status]}</span>
        <OperationStatusIcon status={step.status} />
        <span className="operationStepChevron" aria-hidden="true">⌄</span>
      </button>
      {expanded ? (
        <div className="operationStepDetails" id={contentId}>
          <span className="operationStepDetailsTitle">{copy.details}</span>
          {logs.length > 0 ? logs.map((log) => (
            <div className="operationStepLog" data-level={log.level} key={log.id}>
              <p>{log.message}</p>
              <div className="operationStepMetaGrid">
                <OperationDetail label={copy.command} value={log.command ?? "-"} code />
                <OperationDetail label={copy.exitCode} value={log.exitCode ?? "-"} code />
                <OperationDetail label={copy.duration} value={typeof log.durationMs === "number" ? `${log.durationMs} ms` : "-"} code />
                <OperationDetail
                  label={copy.timedOut}
                  value={typeof log.timedOut === "boolean" ? (log.timedOut ? copy.yes : copy.no) : "-"}
                  code
                />
              </div>
              <div className="operationStepStreamGrid">
                <OperationDetail label={copy.stdout} value={log.stdout || copy.noOutput} pre />
                <OperationDetail label={copy.stderr} value={log.stderr || copy.noOutput} pre />
              </div>
            </div>
          )) : <p className="operationStepEmpty">{copy.noLogs}</p>}
        </div>
      ) : null}
    </section>
  );
}

export function OperationProgressPanel({
  copy,
  hosts,
  resolveStep
}: {
  copy: OperationProgressCopy;
  hosts: OperationProgressHost[];
  resolveStep: (step: TaskStep) => OperationStepPresentation;
}) {
  const [selectedHostAlias, setSelectedHostAlias] = useState(() => hosts[0]?.hostAlias ?? "");
  const hostTabRefs = useRef(new Map<string, HTMLButtonElement>());

  useEffect(() => {
    if (hosts.some((host) => host.hostAlias === selectedHostAlias)) return;
    setSelectedHostAlias(hosts[0]?.hostAlias ?? "");
  }, [hosts, selectedHostAlias]);

  const selectedHost = hosts.find((host) => host.hostAlias === selectedHostAlias) ?? hosts[0];
  if (!selectedHost) return null;
  const selectHostFromKeyboard = (event: KeyboardEvent<HTMLButtonElement>, currentIndex: number) => {
    let nextIndex: number | null = null;
    if (event.key === "ArrowRight") nextIndex = (currentIndex + 1) % hosts.length;
    if (event.key === "ArrowLeft") nextIndex = (currentIndex - 1 + hosts.length) % hosts.length;
    if (event.key === "Home") nextIndex = 0;
    if (event.key === "End") nextIndex = hosts.length - 1;
    if (nextIndex === null) return;
    event.preventDefault();
    const nextAlias = hosts[nextIndex].hostAlias;
    setSelectedHostAlias(nextAlias);
    hostTabRefs.current.get(nextAlias)?.focus();
  };

  return (
    <div className="operationProgressPanel">
      {hosts.length > 1 ? (
        <div aria-label={copy.hostSelector} className="operationHostSelector" role="tablist">
          {hosts.map((host, index) => {
            const selected = host.hostAlias === selectedHost.hostAlias;
            const complete = host.steps.filter((step) => step.status === "success" || step.status === "failed" || step.status === "skipped").length;
            return (
              <button
                aria-controls={selected ? `operation-host-panel-${safeDomId(host.hostAlias)}` : undefined}
                aria-label={`${host.hostName}. ${operationHostStatusLabel(copy, host.status)}. ${complete}/${host.steps.length}`}
                aria-selected={selected}
                className="operationHostTab"
                data-status={host.status}
                id={`operation-host-tab-${safeDomId(host.hostAlias)}`}
                key={host.hostAlias}
                ref={(node) => {
                  if (node) hostTabRefs.current.set(host.hostAlias, node);
                  else hostTabRefs.current.delete(host.hostAlias);
                }}
                role="tab"
                tabIndex={selected ? 0 : -1}
                type="button"
                onClick={() => setSelectedHostAlias(host.hostAlias)}
                onKeyDown={(event) => selectHostFromKeyboard(event, index)}
              >
                <HostStatusDot status={host.status} />
                <span>{host.hostName}</span>
                <small>{complete}/{host.steps.length}</small>
              </button>
            );
          })}
        </div>
      ) : null}
      <div
        aria-labelledby={hosts.length > 1 ? `operation-host-tab-${safeDomId(selectedHost.hostAlias)}` : undefined}
        className="operationSelectedHost"
        id={`operation-host-panel-${safeDomId(selectedHost.hostAlias)}`}
        role={hosts.length > 1 ? "tabpanel" : undefined}
      >
        {hosts.length > 1 ? (
          <div className="operationSelectedHostHeader">
            <strong>{selectedHost.hostName}</strong>
            <span>{selectedHost.message}</span>
          </div>
        ) : null}
        <div className="operationStepList">
          {selectedHost.steps.map((step) => (
            <OperationStepCard
              copy={copy}
              key={`${step.taskRunId}:${step.stepId}`}
              logs={logsForStep(selectedHost.logs, step.stepId)}
              presentation={resolveStep(step)}
              step={step}
            />
          ))}
        </div>
      </div>
    </div>
  );
}

export function OperationProgressModal({
  copy,
  footer,
  hosts,
  message,
  onClose,
  onViewTasks,
  overallStatus,
  resolveStep,
  title
}: {
  copy: OperationProgressCopy;
  footer?: ReactNode;
  hosts: OperationProgressHost[];
  message: string;
  onClose: () => void;
  onViewTasks?: () => void;
  overallStatus: OperationOverallStatus;
  resolveStep: (step: TaskStep) => OperationStepPresentation;
  title: string;
}) {
  const titleId = "operation-progress-modal-title";
  return (
    <div className="modalBackdrop" role="presentation">
      <ModalFrame className="codexOperationModal operationProgressModal" titleId={titleId}>
        <header className="operationProgressHeader">
          <div>
            <h2 id={titleId}>{title}</h2>
            <span className="operationOverallStatus" data-status={overallStatus}>{copy.overallStatus[overallStatus]}</span>
          </div>
          <button className="modalCloseButton" type="button" aria-label={copy.close} onClick={onClose}>×</button>
        </header>
        <p aria-live="polite" className="operationProgressMessage">{message}</p>
        <OperationProgressPanel copy={copy} hosts={hosts} resolveStep={resolveStep} />
        <div className="modalActions codexOperationActions">
          {onViewTasks ? <button className="secondaryButton" type="button" onClick={onViewTasks}>{copy.viewTasks}</button> : null}
          <button className="primaryButton" type="button" onClick={onClose}>
            {overallStatus === "running" ? copy.hide : copy.close}
          </button>
        </div>
        {footer}
      </ModalFrame>
    </div>
  );
}

function OperationDetail({
  code = false,
  label,
  pre = false,
  value
}: {
  code?: boolean;
  label: string;
  pre?: boolean;
  value: ReactNode;
}) {
  return (
    <div>
      <span>{label}</span>
      {pre ? <pre>{value}</pre> : code ? <code>{value}</code> : <span>{value}</span>}
    </div>
  );
}

function HostStatusDot({ status }: { status: OperationProgressHost["status"] }) {
  const stepStatus: TaskStepStatus = status === "pending" || status === "partial"
    ? "pending"
    : status;
  return <span aria-hidden="true" className="operationHostStatusDot" data-status={stepStatus} />;
}

function operationHostStatusLabel(copy: OperationProgressCopy, status: OperationProgressHost["status"]) {
  if (status === "pending") return copy.status.pending;
  if (status === "running") return copy.status.running;
  if (status === "success") return copy.status.success;
  if (status === "failed") return copy.status.failed;
  return copy.overallStatus.partial;
}

function logsForStep(logs: TaskLog[], stepId: string) {
  return logs.filter((log) => log.stepId === stepId);
}

function safeDomId(value: string) {
  return value.replace(/[^a-zA-Z0-9_-]/g, "-");
}
