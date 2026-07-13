import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import type { TaskLog, TaskStep, TaskStepStatus } from "../models";
import {
  OperationProgressModal,
  OperationProgressPanel,
  OperationStepCard,
  finalizeOperationProgressHost,
  mergeOperationProgressHost,
  operationHostStatusFromSteps,
  settleOperationStepsAfterFailure,
  type OperationProgressCopy,
  type OperationProgressHost
} from "./OperationProgress";

const copy: OperationProgressCopy = {
  close: "Close",
  hide: "Hide",
  viewTasks: "View Tasks",
  hostSelector: "Hosts",
  progress: "Progress",
  details: "Technical details",
  noLogs: "No details yet.",
  noOutput: "(no output)",
  command: "Command",
  exitCode: "Exit",
  duration: "Duration",
  timedOut: "Timed out",
  stdout: "stdout",
  stderr: "stderr",
  yes: "Yes",
  no: "No",
  status: {
    pending: "Waiting",
    running: "Running",
    success: "Completed",
    failed: "Failed",
    skipped: "Not required"
  },
  overallStatus: {
    running: "Running",
    success: "Completed",
    failed: "Failed",
    partial: "Partially completed"
  }
};

function step(status: TaskStepStatus, stepId: string = status): TaskStep {
  return {
    taskRunId: "task-1",
    stepId,
    sequence: 0,
    status,
    summary: `${status} summary`,
    startedAt: null,
    endedAt: null
  };
}

const detailLog: TaskLog = {
  id: "log-1",
  taskRunId: "task-1",
  stepId: "failed",
  level: "error",
  timestamp: "2026-07-13T08:00:00Z",
  message: "Official installer failed; trying the mirror.",
  command: "install-codex",
  stdout: "downloaded package",
  stderr: "network unavailable",
  exitCode: 1,
  durationMs: 320,
  timedOut: undefined
};

const resolveStep = (item: TaskStep) => ({ title: item.stepId, summary: item.summary });

test("step cards show all four visual states and stay collapsed, including failures", async () => {
  const user = userEvent.setup();
  const { rerender } = render(
    <div>
      {(["pending", "running", "success", "failed", "skipped"] as TaskStepStatus[]).map((status) => (
        <OperationStepCard
          copy={copy}
          key={status}
          logs={status === "failed" ? [detailLog] : []}
          presentation={resolveStep(step(status))}
          step={step(status)}
        />
      ))}
    </div>
  );

  expect(document.querySelectorAll('.operationStatusIcon[data-status="pending"]')).toHaveLength(1);
  expect(document.querySelectorAll('.operationStatusIcon[data-status="running"]')).toHaveLength(1);
  expect(document.querySelectorAll('.operationStatusIcon[data-status="success"]')).toHaveLength(1);
  expect(document.querySelectorAll('.operationStatusIcon[data-status="failed"]')).toHaveLength(1);
  expect(document.querySelectorAll('.operationStatusIcon[data-status="skipped"]')).toHaveLength(1);
  expect(screen.queryByRole("img")).not.toBeInTheDocument();
  expect(screen.queryByText("install-codex")).not.toBeInTheDocument();
  const failedButton = screen.getByRole("button", { name: /failed.*failed summary.*failed/i });
  expect(failedButton).toHaveAttribute("aria-expanded", "false");
  expect(failedButton.querySelector(".operationStepText")?.compareDocumentPosition(failedButton.querySelector(".operationStatusIcon")!))
    .toBe(Node.DOCUMENT_POSITION_FOLLOWING);

  await user.click(failedButton);
  expect(screen.getByText("install-codex")).toBeInTheDocument();
  expect(screen.getByText("network unavailable")).toBeInTheDocument();
  expect(screen.getByText("-", { selector: "code" })).toBeInTheDocument();

  rerender(
    <OperationStepCard
      copy={copy}
      logs={[detailLog]}
      presentation={resolveStep(step("failed"))}
      step={step("failed")}
    />
  );
  expect(screen.queryByText("install-codex")).not.toBeInTheDocument();
});

test("panel supports multiple running steps and keeps the selected host stable", async () => {
  const user = userEvent.setup();
  const hosts: OperationProgressHost[] = [
    {
      hostAlias: "alpha",
      hostName: "Alpha",
      status: "running",
      steps: [step("running", "system"), step("running", "codex")],
      logs: []
    },
    {
      hostAlias: "beta",
      hostName: "Beta",
      status: "failed",
      steps: [step("failed", "ssh")],
      logs: []
    }
  ];
  const { rerender } = render(<OperationProgressPanel copy={copy} hosts={hosts} resolveStep={resolveStep} />);
  expect(document.querySelectorAll('.operationStatusIcon[data-status="running"]')).toHaveLength(2);

  const alphaTab = screen.getByRole("tab", { name: "Alpha. Running. 0/2" });
  const betaTab = screen.getByRole("tab", { name: "Beta. Failed. 1/1" });
  expect(alphaTab).toHaveAttribute("aria-controls", "operation-host-panel-alpha");
  expect(betaTab).not.toHaveAttribute("aria-controls");
  await user.click(betaTab);
  expect(screen.getByRole("tab", { name: "Beta. Failed. 1/1" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByRole("tab", { name: "Beta. Failed. 1/1" })).toHaveAttribute("aria-controls", "operation-host-panel-beta");
  expect(screen.getByRole("tab", { name: "Alpha. Running. 0/2" })).not.toHaveAttribute("aria-controls");

  await user.keyboard("{ArrowLeft}");
  expect(screen.getByRole("tab", { name: "Alpha. Running. 0/2" })).toHaveAttribute("aria-selected", "true");
  await user.keyboard("{End}");
  expect(screen.getByRole("tab", { name: "Beta. Failed. 1/1" })).toHaveAttribute("aria-selected", "true");

  rerender(
    <OperationProgressPanel
      copy={copy}
      hosts={[{ ...hosts[0], status: "success" }, hosts[1]]}
      resolveStep={resolveStep}
    />
  );
  expect(screen.getByRole("tab", { name: "Beta. Failed. 1/1" })).toHaveAttribute("aria-selected", "true");
  expect(screen.getByText("ssh")).toBeInTheDocument();
});

test("overall running state is controlled independently from successful steps", () => {
  render(
    <OperationProgressModal
      copy={copy}
      hosts={[{
        hostAlias: "alpha",
        hostName: "Alpha",
        status: "running",
        steps: [step("success", "preflight")],
        logs: []
      }]}
      message="Final verification is still running."
      overallStatus="running"
      resolveStep={resolveStep}
      title="Codex maintenance"
      onClose={() => undefined}
    />
  );

  const dialog = screen.getByRole("dialog", { name: "Codex maintenance" });
  expect(within(dialog).getByText("Running", { selector: ".operationOverallStatus" })).toBeInTheDocument();
  expect(within(dialog).getByText("Completed", { selector: ".operationStepState" })).toBeInTheDocument();
  expect(within(dialog).getByRole("button", { name: "Hide" })).toBeInTheDocument();
});

test("host status waits for the operation-specific terminal step instead of failing on a recovered method", () => {
  const fallbackSteps = [
    step("success", "preparation"),
    step("failed", "official-installer"),
    step("failed", "remote-native-mirror"),
    step("success", "remote-npm-mirror"),
    step("skipped", "local-upload"),
    step("success", "final-verification")
  ];
  expect(operationHostStatusFromSteps("codex-install", fallbackSteps)).toBe("success");
  expect(operationHostStatusFromSteps("codex-update", fallbackSteps.slice(0, -1).concat(step("running", "final-verification")))).toBe("running");
  expect(operationHostStatusFromSteps("host-test", [step("success", "ssh-check"), step("failed", "system"), step("skipped", "codex")])).toBe("failed");
  expect(operationHostStatusFromSteps("codex-uninstall", [
    step("failed", "preparation"),
    step("skipped", "uninstall"),
    step("skipped", "final-verification")
  ])).toBe("failed");
});

test("batch invocation failure settles every running and waiting step", () => {
  const settled = settleOperationStepsAfterFailure([
    step("success", "ssh-check"),
    step("running", "system"),
    step("running", "codex"),
    step("pending", "api")
  ], "Batch command failed.", "2026-07-13T10:00:00Z");

  expect(settled.map((item) => item.status)).toEqual(["success", "failed", "failed", "skipped"]);
  expect(settled.slice(1).every((item) => item.summary === "Batch command failed.")).toBe(true);
});

test("late progress events cannot regress terminal steps or a finalized host", () => {
  const terminalHost: OperationProgressHost = {
    hostAlias: "alpha",
    hostName: "Alpha",
    status: "success",
    steps: [step("success", "final-verification")],
    logs: []
  };
  const lateRunning = step("running", "final-verification");
  const merged = mergeOperationProgressHost(terminalHost, "codex-update", lateRunning);
  expect(merged.status).toBe("success");
  expect(merged.steps[0].status).toBe("success");

  const finalTask = {
    id: "task-final",
    hostId: "alpha",
    hostName: "Alpha",
    action: "Update Codex",
    status: "success" as const,
    startedAt: "2026-07-13T10:00:00Z",
    endedAt: "2026-07-13T10:00:03Z",
    summary: "Update completed.",
    steps: [step("success", "preparation"), step("success", "final-verification")],
    logs: [{ ...detailLog, id: "final-log", stepId: "final-verification" }]
  };
  const finalized = finalizeOperationProgressHost(terminalHost, finalTask, true, finalTask.summary);
  expect(finalized.finalized).toBe(true);
  expect(finalized.steps).toEqual(finalTask.steps);
  expect(finalized.logs).toEqual(finalTask.logs);
  expect(mergeOperationProgressHost(finalized, "codex-update", step("failed", "final-verification"), detailLog)).toBe(finalized);
});
