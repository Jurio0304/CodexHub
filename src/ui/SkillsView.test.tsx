import { render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { SkillsView, uiCopy } from "../App";
import type {
  InstalledSkillDownloadResult,
  SkillInventoryStatus,
  SkillPack,
  SkillTargetOperationResult,
  SkillTargetsResult
} from "../models";
import { FeedbackProvider } from "./feedback";

const librarySkill: SkillPack = {
  id: "example-skill",
  name: "Example skill",
  version: "1.0.0",
  description: "Example description",
  about: "Example description",
  sourceType: "local",
  source: "C:\\skills\\example-skill",
  originalPath: "C:\\skills\\example-skill",
  managedPath: "C:\\CodexHub\\skills\\example-skill",
  hasSkillMd: true,
  skillCount: 1,
  enabled: true,
  addedAt: "2026-07-11",
  updatedAt: "2026-07-11",
  applications: [{
    targetType: "local",
    label: "Local machine",
    hostAlias: null,
    path: "C:\\Users\\PC\\.codex\\skills\\example-skill",
    detectedAt: "2026-07-11",
    hasSkillMd: true
  }]
};

const inventoryStatus: SkillInventoryStatus = {
  firstHostScanCompleted: true,
  localSkillRoot: "C:\\Users\\PC\\.codex\\skills",
  localSkills: [{
    name: "Remote only",
    path: "C:\\Users\\PC\\.codex\\skills\\remote-only",
    hasSkillMd: true,
    status: "installed",
    description: "Installed-only skill"
  }],
  hostInventories: []
};

const targetResult: SkillTargetsResult = {
  skillId: librarySkill.id,
  skillName: librarySkill.name,
  targets: [{
    targetType: "local",
    label: "Local machine",
    hostAlias: null,
    path: "C:\\Users\\PC\\.codex\\skills\\example-skill",
    installed: true,
    canInstall: false,
    canUninstall: true,
    status: "installed",
    message: "Installed"
  }],
  tasks: [],
  message: "Ready"
};

const operationResult: SkillTargetOperationResult = {
  ok: true,
  skills: [librarySkill],
  tasks: [],
  results: [],
  message: "uninstall-success"
};

function renderSkills() {
  const onDeleteLibrarySkill = vi.fn(async () => operationResult);
  const onDownloadInstalledSkill = vi.fn(async (): Promise<InstalledSkillDownloadResult> => ({
    imported: [librarySkill],
    skipped: [],
    skills: [librarySkill],
    status: inventoryStatus,
    tasks: [],
    message: "Downloaded"
  }));
  const onUninstallInstalledSkill = vi.fn(async () => operationResult);
  const onUninstallSkillTargets = vi.fn(async () => operationResult);

  render(
    <FeedbackProvider>
      <SkillsView
        copy={uiCopy.en}
        hosts={[]}
        inventoryStatus={inventoryStatus}
        skillPacks={[librarySkill]}
        onDeleteLibrarySkill={onDeleteLibrarySkill}
        onDetectInstalledSkills={async () => ({ skills: [librarySkill], status: inventoryStatus, tasks: [], message: "Detected" })}
        onDownloadInstalledSkill={onDownloadInstalledSkill}
        onDownloadGithubSkill={async () => ({ imported: [], skipped: [], message: "Downloaded" })}
        onGetSkillTargets={async () => targetResult}
        onImportSkillDirectory={async () => null}
        onInstallSkillTargets={async () => operationResult}
        onRefreshSkillLibrary={async () => undefined}
        onUninstallInstalledSkill={onUninstallInstalledSkill}
        onUninstallSkillTargets={onUninstallSkillTargets}
        onUpdateLibrarySkillAbout={async () => librarySkill}
        onViewTasks={() => undefined}
      />
    </FeedbackProvider>
  );

  return {
    onDeleteLibrarySkill,
    onDownloadInstalledSkill,
    onUninstallInstalledSkill,
    onUninstallSkillTargets
  };
}

test("skill download and delete buttons open usable dialogs", async () => {
  const user = userEvent.setup();
  const { onDeleteLibrarySkill } = renderSkills();

  await user.click(screen.getByRole("button", { name: "Download" }));
  const downloadDialog = await screen.findByRole("dialog", { name: "Download skill" });
  expect(within(downloadDialog).getByRole("textbox")).toBeVisible();
  const cancelButtons = within(downloadDialog).getAllByRole("button", { name: "Cancel" });
  await user.click(cancelButtons[cancelButtons.length - 1]);

  await user.click(screen.getByRole("button", { name: "Delete" }));
  const deleteDialog = await screen.findByRole("alertdialog", { name: "Delete skill: Example skill" });
  await user.click(within(deleteDialog).getByRole("button", { name: "Delete only" }));
  expect(onDeleteLibrarySkill).toHaveBeenCalledWith(librarySkill.id, false);
});

test("library uninstall confirmation executes the selected target operation", async () => {
  const user = userEvent.setup();
  const { onUninstallSkillTargets } = renderSkills();

  await user.click(screen.getByRole("button", { name: "Uninstall" }));
  const dialog = await screen.findByRole("alertdialog", { name: "Uninstall: Example skill" });
  await user.click(within(dialog).getByRole("checkbox"));
  await user.click(within(dialog).getByRole("button", { name: "Uninstall" }));

  expect(onUninstallSkillTargets).toHaveBeenCalledWith(librarySkill.id, [{ targetType: "local", hostAlias: null }]);
});

test("installed skill download and uninstall keep their second confirmations", async () => {
  const user = userEvent.setup();
  const { onDownloadInstalledSkill, onUninstallInstalledSkill } = renderSkills();

  await user.click(screen.getByRole("button", { name: "Remote only" }));
  let preview = await screen.findByRole("dialog", { name: "Remote only" });
  await user.click(within(preview).getByRole("button", { name: "Download" }));
  let confirmation = await screen.findByRole("alertdialog", { name: "Download installed skill" });
  await user.click(within(confirmation).getByRole("button", { name: "Download to library" }));
  expect(onDownloadInstalledSkill).toHaveBeenCalledTimes(1);

  const operationDialog = screen.getByRole("dialog", { name: "Download installed skill" });
  const closeButtons = within(operationDialog).getAllByRole("button", { name: "Close" });
  await user.click(closeButtons[closeButtons.length - 1]);
  await user.click(screen.getByRole("button", { name: "Remote only" }));
  preview = await screen.findByRole("dialog", { name: "Remote only" });
  await user.click(within(preview).getByRole("button", { name: "Uninstall" }));
  confirmation = await screen.findByRole("alertdialog", { name: "Uninstall installed skill" });
  await user.click(within(confirmation).getByRole("button", { name: "Uninstall from this target" }));
  expect(onUninstallInstalledSkill).toHaveBeenCalledTimes(1);
});
