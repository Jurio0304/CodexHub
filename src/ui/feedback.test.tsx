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
    </>
  );
}

const localizedLabels: FeedbackLabels = {
  persistentRegion: "Bleibende Fehler",
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

test("transient feedback uses Toast and errors remain persistent", async () => {
  const user = userEvent.setup();
  render(<FeedbackProvider><Harness /></FeedbackProvider>);

  await user.click(screen.getByRole("button", { name: "Success" }));
  expect(await screen.findByText("Saved")).toBeVisible();

  await user.click(screen.getByRole("button", { name: "Error" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Write failed");
  expect(screen.getByText("Write failed")).toBeVisible();
});

test("feedback surface has no axe violations", async () => {
  const user = userEvent.setup();
  render(<FeedbackProvider><Harness /></FeedbackProvider>);
  await user.click(screen.getByRole("button", { name: "Error" }));
  await waitFor(() => expect(screen.getByRole("alert")).toBeVisible());
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations).toEqual([]);
});

test("task-linked errors use configured labels and open the durable task", async () => {
  const user = userEvent.setup();
  const onOpenTask = vi.fn();
  render(<FeedbackProvider><ConfiguredHarness onOpenTask={onOpenTask} /></FeedbackProvider>);

  await user.click(screen.getByRole("button", { name: "Task error" }));
  expect(await screen.findByRole("complementary", { name: "Bleibende Fehler" })).toBeVisible();
  await user.click(screen.getByRole("button", { name: "Aufgabe anzeigen" }));

  expect(onOpenTask).toHaveBeenCalledWith("task-42");
  expect(screen.queryByText("Persisted failure")).not.toBeInTheDocument();
});

test("success Toast closes after its five-second lifetime", async () => {
  vi.useFakeTimers();
  try {
    render(<FeedbackProvider><Harness /></FeedbackProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Success" }));
    expect(screen.getByText("Saved")).toBeVisible();

    await act(async () => {
      await vi.advanceTimersByTimeAsync(5100);
    });
    expect(screen.queryByText("Saved")).not.toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});
