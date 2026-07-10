import axe from "axe-core";
import { useState } from "react";
import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test } from "vitest";
import { ModalFrame } from "./ModalFrame";
import { AlertModalFrame } from "./AlertModalFrame";

function FormHarness({ busy = false, destructive = false }: { busy?: boolean; destructive?: boolean }) {
  const [open, setOpen] = useState(false);
  return (
    <>
      <button type="button" onClick={() => setOpen(true)}>Open</button>
      {open && destructive ? (
        <AlertModalFrame busy={busy} className="simpleDeleteModal" titleId="test-title" onCancel={() => setOpen(false)}>
          <h2 id="test-title">Dialog title</h2>
          <div className="modalActions">
            <button className="secondaryButton" data-alert-cancel disabled={busy} type="button" onClick={() => setOpen(false)}>Cancel</button>
            <button className="dangerButton" type="button">Delete</button>
          </div>
        </AlertModalFrame>
      ) : open ? (
        <div className="modalBackdrop">
          <ModalFrame className="formModal" titleId="test-title">
            <h2 id="test-title">Dialog title</h2>
            <input aria-label="Name" />
            <div className="modalActions">
              <button className="secondaryButton modalCloseButton" disabled={busy} type="button" onClick={() => setOpen(false)}>Cancel</button>
              <button type="button">Save</button>
            </div>
          </ModalFrame>
        </div>
      ) : null}
    </>
  );
}

test("form dialogs focus the first editor, close on Escape, and restore focus", async () => {
  const user = userEvent.setup();
  render(<FormHarness />);
  const trigger = screen.getByRole("button", { name: "Open" });
  await user.click(trigger);
  await waitFor(() => expect(screen.getByRole("textbox", { name: "Name" })).toHaveFocus());
  await user.tab();
  expect(screen.getByRole("dialog")).toContainElement(document.activeElement as HTMLElement);
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("dialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});

test("busy dialogs block Escape and destructive dialogs start on Cancel", async () => {
  const user = userEvent.setup();
  const { unmount } = render(<FormHarness busy destructive />);
  await user.click(screen.getByRole("button", { name: "Open" }));
  await user.keyboard("{Escape}");
  expect(screen.getByRole("alertdialog")).toBeVisible();

  unmount();
  render(<FormHarness destructive />);
  const trigger = screen.getByRole("button", { name: "Open" });
  await user.click(trigger);
  await waitFor(() => expect(screen.getByRole("button", { name: "Cancel" })).toHaveFocus());
  expect(screen.getByRole("alertdialog")).toBeVisible();
  await user.keyboard("{Escape}");
  await waitFor(() => expect(screen.queryByRole("alertdialog")).not.toBeInTheDocument());
  expect(trigger).toHaveFocus();
});

test("Tab and Shift+Tab remain inside the top-level dialog", async () => {
  const user = userEvent.setup();
  render(<FormHarness />);
  await user.click(screen.getByRole("button", { name: "Open" }));
  const dialog = screen.getByRole("dialog");
  for (let index = 0; index < 6; index += 1) {
    await user.tab();
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
  }
  for (let index = 0; index < 3; index += 1) {
    await user.tab({ shift: true });
    expect(dialog).toContainElement(document.activeElement as HTMLElement);
  }
});

test("dialog primitives have no axe violations", async () => {
  const user = userEvent.setup();
  render(<FormHarness destructive />);
  await user.click(screen.getByRole("button", { name: "Open" }));
  await waitFor(() => expect(screen.getByRole("alertdialog")).toBeVisible());
  const result = await axe.run(document.body, { rules: { "color-contrast": { enabled: false } } });
  expect(result.violations).toEqual([]);
});
