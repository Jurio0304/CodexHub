import axe from "axe-core";
import { useEffect } from "react";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { FeedbackProvider, useFeedback } from "./feedback";
import type { FeedbackLabels } from "./feedback";

function Harness() {
  const { notify } = useFeedback();
  return (
    <>
      <button type="button" onClick={() => notify({ message: "Saved", tone: "success" })}>Success</button>
      <button type="button" onClick={() => notify({ message: "Write failed", tone: "error" })}>Error</button>
      <button type="button" onClick={() => notify({ message: "Dialog opened", placement: "global", tone: "info" })}>Global info</button>
    </>
  );
}

const localizedLabels: FeedbackLabels = {
  retry: "Erneut",
  details: "Details",
  viewTask: "Aufgabe anzeigen",
  dismiss: "Schließen",
  notifications: "Hinweise"
};

function ConfiguredHarness({ onOpenTask }: { onOpenTask: (taskId: string) => void }) {
  const { configure, notify } = useFeedback();
  useEffect(() => {
    configure({ labels: localizedLabels, onOpenTask });
  }, [configure, onOpenTask]);
  return (
    <button type="button" onClick={() => notify({ message: "Persisted failure", taskId: "task-42", tone: "error" })}>
      Task error
    </button>
  );
}

test("success and error feedback share the transient Toast surface without a close button", async () => {
  const user = userEvent.setup();
  render(<FeedbackProvider><Harness /></FeedbackProvider>);

  await user.click(screen.getByRole("button", { name: "Success" }));
  const success = await screen.findByText("Saved");
  expect(success).toBeVisible();
  expect(success.closest("[data-tone]")).toHaveAttribute("data-tone", "success");

  await user.click(screen.getByRole("button", { name: "Error" }));
  const failure = await screen.findByText("Write failed");
  expect(failure).toBeVisible();
  expect(failure.closest("[data-tone]")).toHaveAttribute("data-tone", "error");
  expect(screen.queryByRole("button", { name: "Dismiss" })).not.toBeInTheDocument();
});

test("feedback surface has no axe violations", async () => {
  const user = userEvent.setup();
  render(<FeedbackProvider><Harness /></FeedbackProvider>);
  await user.click(screen.getByRole("button", { name: "Error" }));
  await waitFor(() => expect(screen.getByText("Write failed")).toBeVisible());
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations).toEqual([]);
});

test("global information feedback uses the global viewport placement", async () => {
  const user = userEvent.setup();
  render(<FeedbackProvider><Harness /></FeedbackProvider>);

  await user.click(screen.getByRole("button", { name: "Global info" }));
  const info = await screen.findByText("Dialog opened");
  expect(info.closest("[data-tone]")).toHaveAttribute("data-tone", "info");
  expect(document.querySelector(".feedbackToastViewport")).toHaveAttribute("data-placement", "global");
});

test("task-linked errors use configured labels and open the durable task", async () => {
  const user = userEvent.setup();
  const onOpenTask = vi.fn();
  render(<FeedbackProvider><ConfiguredHarness onOpenTask={onOpenTask} /></FeedbackProvider>);

  await user.click(screen.getByRole("button", { name: "Task error" }));
  expect(await screen.findByText("Persisted failure")).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Aufgabe anzeigen" }));

  expect(onOpenTask).toHaveBeenCalledWith("task-42");
  expect(screen.queryByText("Persisted failure")).not.toBeInTheDocument();
});

test("all Toast tones close after their lifetime and one-second exit", async () => {
  vi.useFakeTimers();
  try {
    render(<FeedbackProvider><Harness /></FeedbackProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Success" }));
    fireEvent.click(screen.getByRole("button", { name: "Error" }));
    expect(screen.getByText("Saved")).toBeVisible();
    expect(screen.getByText("Write failed")).toBeVisible();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(6100);
    });
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
    expect(screen.queryByText("Write failed")).not.toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});

test("the next screen interaction dismisses visible feedback", async () => {
  vi.useFakeTimers();
  try {
    render(<FeedbackProvider><Harness /></FeedbackProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Success" }));
    expect(screen.getByText("Saved")).toBeVisible();

    fireEvent.wheel(document);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1100);
    });
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});
