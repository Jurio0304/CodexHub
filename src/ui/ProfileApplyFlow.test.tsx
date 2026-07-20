import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import { ProfilesView, profileApplyOperationPresentation, uiCopy } from "../App";
import type { ProfileApplyOperationStatus } from "../App";
import type {
  Host,
  Profile,
  ProfileApplyBatchResult,
  ProfileApplyOptions,
  ProfileApplyOutcome,
  ProfileApplyPreview,
  TaskRun
} from "../models";
import { FeedbackProvider } from "./feedback";

const testProfile: Profile = {
  id: "profile-reload-test",
  name: "Reload test",
  description: "Profile reload test",
  model: "gpt-5-codex",
  provider: "openai",
  baseUrl: "https://api.openai.com/v1",
  apiKeyEnvVar: "OPENAI_API_KEY",
  modelReasoningEffort: "medium",
  planModeReasoningEffort: "high",
  fastMode: false,
  serviceTier: "auto",
  approvalPolicy: "on-request",
  sandboxMode: "workspace-write",
  extraToml: "",
  createdAt: "2026-07-17T00:00:00.000Z",
  updatedAt: "2026-07-17T00:00:00.000Z",
  source: "mock",
  credentialStored: false,
  hostIds: []
};

const testHost: Host = {
  id: "host-reload-test",
  name: "Reload host",
  hostAlias: "reload-host",
  source: "mock",
  address: "192.0.2.10",
  port: 22,
  username: "codex",
  authMethod: "ssh-key",
  status: "online",
  os: "Debian 12",
  arch: "x86_64",
  shell: "/bin/sh",
  path: "/usr/local/bin:/usr/bin",
  pathHasLocalBin: true,
  codexCommandAvailable: true,
  codexInstalled: true,
  codexVersion: "codex-cli 0.1.0",
  configExists: false,
  apiConfigName: null,
  apiConfigSource: null,
  apiKeyEnvVar: null,
  apiKeyEnvPresent: null,
  skillsExists: false,
  skillsCount: 0,
  profileId: null,
  skillPackIds: [],
  tags: [],
  lastSeen: "just now",
  latencyMs: 12
};

const secondTestHost: Host = {
  ...testHost,
  id: "host-reload-test-2",
  name: "Reload host 2",
  hostAlias: "reload-host-2",
  address: "192.0.2.11"
};

const confirmedProfileHost: Host = {
  ...secondTestHost,
  id: "host-confirmed-profile",
  name: "Confirmed profile host",
  hostAlias: "confirmed-profile-host",
  address: "192.0.2.12",
  configExists: true,
  apiConfigName: testProfile.name,
  apiConfigSource: "profile",
  profileId: testProfile.id
};

const locallyLinkedUnconfirmedHost: Host = {
  ...secondTestHost,
  id: "host-local-link-only",
  name: "Local link only",
  hostAlias: "local-link-only",
  address: "192.0.2.13",
  status: "unknown",
  codexVersion: "pending",
  configExists: null,
  apiConfigName: null,
  apiConfigSource: null,
  profileId: testProfile.id
};

const manualReconnectTask: TaskRun = {
  id: "task-profile-manual-reconnect",
  hostId: testHost.id,
  hostName: testHost.name,
  action: "Apply profile",
  status: "failed",
  startedAt: "2026-07-17T00:00:00.000Z",
  endedAt: "2026-07-17T00:00:01.000Z",
  summary: "Configuration applied; manual reconnect required.",
  steps: [],
  logs: []
};

test.each<[ProfileApplyOperationStatus, string, string, boolean]>([
  ["running", "Running", "blue", false],
  ["success", "Completed", "green", false],
  ["partial", "Partially completed", "yellow", true],
  ["manual-reconnect", "Manual reconnect required", "yellow", true],
  ["failed", "Failed", "red", false]
])("profile apply outcome %s has the expected result presentation", (status, label, tone, showManualGuide) => {
  expect(profileApplyOperationPresentation(uiCopy.en, status)).toEqual({ label, tone, showManualGuide });
});

function renderProfilesView({
  hosts = [testHost],
  outcome = "success"
}: {
  hosts?: Host[];
  outcome?: ProfileApplyOutcome;
} = {}) {
  const profile = { ...testProfile, hostIds: [] };
  const renderedHosts = hosts.map((host) => ({ ...host }));
  const preview: ProfileApplyPreview = {
    profileId: profile.id,
    profileName: profile.name,
    renderedToml: 'model = "gpt-5-codex"',
    targetFiles: renderedHosts.map((host) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      path: "~/.codex/config.toml",
      backupExpected: false,
      noChangeExpected: false
    })),
    hostResults: [],
    warnings: []
  };
  const apply = vi.fn(async (_profileId: string, hostIds: string[], options: ProfileApplyOptions): Promise<ProfileApplyBatchResult> => ({
    profileId: profile.id,
    ok: outcome === "success",
    outcome,
    results: renderedHosts.filter((host) => hostIds.includes(host.id)).map((host) => ({
      hostId: host.id,
      hostName: host.name,
      hostAlias: host.hostAlias,
      status: "success",
      targetPath: "~/.codex/config.toml",
      backupPath: null,
      message: "Configuration applied.",
      reload: {
        mode: options.remoteCodexReloadMode,
        status: outcome === "manual-reconnect"
          ? "manual-required"
          : options.remoteCodexReloadMode === "none" ? "not-requested" : "reconnected",
        targetedCount: options.remoteCodexReloadMode === "none" ? 0 : 1,
        stoppedCount: options.remoteCodexReloadMode === "none" ? 0 : 1,
        preservedCliCount: options.remoteCodexReloadMode === "app-services" ? 1 : 0,
        replacementObserved: outcome !== "manual-reconnect" && options.remoteCodexReloadMode !== "none",
        message: outcome === "manual-reconnect" ? "No replacement appeared within 15 seconds." : "Reload completed."
      }
    })),
    tasks: outcome === "manual-reconnect" ? [manualReconnectTask] : [],
    profiles: [profile],
    hosts: renderedHosts
  }));
  const previewApply = vi.fn(async () => preview);
  const noopAsync = vi.fn(async () => undefined as never);
  const onOpenTask = vi.fn();

  render(
    <FeedbackProvider>
      <ProfilesView
        copy={uiCopy.en}
        hosts={renderedHosts}
        latestCodexVersion={null}
        profiles={[profile]}
        newProfileRequest={0}
        onCreateProfile={noopAsync}
        onDeleteProfile={noopAsync}
        onDetectCcSwitchProfiles={noopAsync}
        onDuplicateProfile={noopAsync}
        onGetProfileApiKey={noopAsync}
        onImportCcSwitchProfiles={noopAsync}
        onImportProfiles={noopAsync}
        onOpenTask={onOpenTask}
        onPreviewProfileApply={previewApply}
        onRunProfileApply={apply}
        onSetProfileApiKey={noopAsync}
        onUpdateProfile={noopAsync}
      />
    </FeedbackProvider>
  );

  return { apply, onOpenTask, previewApply };
}

test("single-host apply resets to App services and gates all-session termination", async () => {
  const user = userEvent.setup();
  const { apply } = renderProfilesView();

  await user.click(screen.getByRole("button", { name: "Apply" }));
  let confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  expect(confirm).toHaveClass("modalFrame", "portalModalContent", "sshHostModal", "profileApplyConfirmModal");
  const appServices = within(confirm).getByRole("radio", { name: /Reload Codex App services/ });
  expect(appServices).toBeChecked();
  expect(apply).not.toHaveBeenCalled();

  await user.click(within(confirm).getByRole("radio", { name: /Apply configuration only/ }));
  await user.click(within(confirm).getByRole("button", { name: "Cancel" }));
  await waitFor(() => expect(screen.queryByRole("alertdialog", { name: "Apply configuration" })).not.toBeInTheDocument());

  await user.click(screen.getByRole("button", { name: "Apply" }));
  confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  expect(within(confirm).getByRole("radio", { name: /Reload Codex App services/ })).toBeChecked();

  await user.click(within(confirm).getByRole("radio", { name: /Stop all remote Codex sessions/ }));
  const submit = within(confirm).getByRole("button", { name: "Apply configuration" });
  expect(submit).toBeDisabled();
  await user.click(within(confirm).getByRole("checkbox", { name: /I understand existing CLI/ }));
  expect(submit).toBeEnabled();
  await user.click(submit);

  await waitFor(() => expect(apply).toHaveBeenCalledWith(
    testProfile.id,
    [testHost.id],
    { remoteCodexReloadMode: "all-codex" }
  ));
});

test("single-host apply explicitly submits the no-reload option", async () => {
  const user = userEvent.setup();
  const { apply } = renderProfilesView();

  await user.click(screen.getByRole("button", { name: "Apply" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  await user.click(within(confirm).getByRole("radio", { name: /Apply configuration only/ }));
  await user.click(within(confirm).getByRole("button", { name: "Apply configuration" }));

  await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
  expect(apply).toHaveBeenCalledWith(
    testProfile.id,
    [testHost.id],
    { remoteCodexReloadMode: "none" }
  );
});

test("batch host selection disables only confirmed matches and counts eligible defaults consistently", async () => {
  const user = userEvent.setup();
  const { apply } = renderProfilesView({
    hosts: [confirmedProfileHost, locallyLinkedUnconfirmedHost, testHost]
  });

  await user.click(screen.getByRole("button", { name: "Select hosts" }));
  const picker = await screen.findByRole("dialog", { name: "Select hosts" });
  const confirmed = within(picker).getByRole("checkbox", { name: /Confirmed profile host/ });
  const localLinkOnly = within(picker).getByRole("checkbox", { name: /Local link only/ });
  const available = within(picker).getByRole("checkbox", { name: /Reload host/ });
  const next = within(picker).getByRole("button", { name: "Next" });

  expect(confirmed).toBeDisabled();
  expect(confirmed).not.toBeChecked();
  expect(localLinkOnly).toBeEnabled();
  expect(localLinkOnly).not.toBeChecked();
  expect(available).toBeEnabled();
  expect(available).not.toBeChecked();
  expect(next).toBeDisabled();

  await user.click(within(picker).getByRole("button", { name: "Select all" }));
  expect(confirmed).not.toBeChecked();
  expect(localLinkOnly).toBeChecked();
  expect(available).toBeChecked();
  expect(next).toBeEnabled();
  await user.click(next);

  const confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  expect(confirm).toHaveTextContent("Apply Reload test to 2 selected hosts");
  expect(apply).not.toHaveBeenCalled();
  await user.click(within(confirm).getByRole("button", { name: "Apply configuration" }));

  await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
  expect(apply).toHaveBeenCalledWith(
    testProfile.id,
    [locallyLinkedUnconfirmedHost.id, testHost.id],
    { remoteCodexReloadMode: "app-services" }
  );
});

test("TOML preview preserves its host ID and options through the confirmation gate", async () => {
  const user = userEvent.setup();
  const { apply, previewApply } = renderProfilesView();

  await user.click(screen.getByRole("button", { name: "Preview" }));
  const preview = await screen.findByRole("dialog", { name: "Preview" });
  await waitFor(() => expect(previewApply).toHaveBeenCalled());
  await user.click(await within(preview).findByRole("button", { name: "Next" }));

  const confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  expect(apply).not.toHaveBeenCalled();
  await user.click(within(confirm).getByRole("radio", { name: /Apply configuration only/ }));
  await user.click(within(confirm).getByRole("button", { name: "Apply configuration" }));

  await waitFor(() => expect(apply).toHaveBeenCalledTimes(1));
  expect(apply).toHaveBeenCalledWith(
    testProfile.id,
    [testHost.id],
    { remoteCodexReloadMode: "none" }
  );
});

test("manual reconnect result keeps the saved-config guide and opens its failed task", async () => {
  const user = userEvent.setup();
  const { onOpenTask } = renderProfilesView({ outcome: "manual-reconnect" });

  await user.click(screen.getByRole("button", { name: "Apply" }));
  const confirm = await screen.findByRole("alertdialog", { name: "Apply configuration" });
  await user.click(within(confirm).getByRole("button", { name: "Apply configuration" }));

  const guide = await screen.findByRole("status");
  expect(guide).toHaveTextContent("Successfully saved configuration remains active.");
  expect(guide).toHaveTextContent("Settings > Codex > Connections");
  await user.click(screen.getByRole("button", { name: "View task" }));

  expect(onOpenTask).toHaveBeenCalledTimes(1);
  expect(onOpenTask).toHaveBeenCalledWith(manualReconnectTask.id);
});
