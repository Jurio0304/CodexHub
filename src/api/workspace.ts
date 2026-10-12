import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  HostDto,
  RemoteFileEntryDto,
  WorkspaceAttachTerminalResultDto,
  WorkspaceFilePreviewDto,
  WorkspaceFileOperationResultDto,
  WorkspaceFileSearchUpdatedEventDto,
  WorkspaceFileSessionDto,
  WorkspaceListDirectoryResultDto,
  WorkspaceLocalPathGrantDto,
  WorkspaceLocalTransferRecoveryDto,
  WorkspacePreparedLocalTransferRecoveryPurgeDto,
  WorkspacePreparedFileOperationDto,
  WorkspacePreparedRecoveryPurgeDto,
  WorkspaceRecoveryDto,
  WorkspaceSessionHeartbeatEventDto,
  WorkspaceSessionStateEventDto,
  WorkspaceTerminalCwdEventDto,
  WorkspaceTerminalOutputEventDto,
  WorkspaceTerminalSessionDto,
  WorkspaceValidatedCwdDto,
  WorkspaceTransferDto,
  WorkspaceTransferSnapshotDto,
  WorkspaceTransferUpdatedEventDto,
} from "../generated/rust-contracts";
import type { TauriCommand } from "./commands";
import { assertTauriRuntime, requiredInvoke } from "./invoke";
import type {
  RemoteFileEntry,
  TerminalAttachment,
  WorkspaceApi,
  WorkspaceFilesSession,
  WorkspaceLocalGrant,
  WorkspaceLocalTransferRecovery,
  WorkspaceRecovery,
  WorkspaceTerminalSession,
  WorkspaceTransfer
} from "../workspace/types";

// Tauri JSON carries these values as numbers today; byte-oriented UI fields
// immediately become decimal text so rendering never relies on JS arithmetic.
const numberValue = (value: unknown): number => typeof value === "bigint" ? Number(value) : Number(value ?? 0);
const decimalText = (value: unknown): string => typeof value === "bigint" ? value.toString() : `${value ?? 0}`;

function terminal(raw: WorkspaceTerminalSessionDto): WorkspaceTerminalSession {
  return {
    sessionId: raw.sessionId,
    hostAlias: raw.hostAlias,
    title: raw.hostName || raw.hostAlias,
    generation: raw.generation,
    revision: numberValue(raw.revision),
    state: raw.state,
    reconnectable: raw.reconnectable,
    autoReconnect: raw.autoReconnect,
    attempt: raw.attempt,
    nextRetryAt: null,
    reason: raw.reason,
    createdAt: raw.createdAt,
    taskId: raw.taskId
  };
}

function entry(raw: RemoteFileEntryDto): RemoteFileEntry {
  return {
    entryRef: raw.entryRef,
    canonicalPath: raw.path,
    name: raw.name,
    kind: raw.kind === "directory" ? "directory" : raw.kind === "file" ? "file" : raw.kind === "symlink" ? "symlink" : "other",
    size: raw.size ?? "",
    modifiedAt: raw.modifiedAt,
    permissions: raw.permissions,
    uid: raw.uid === null ? null : `${raw.uid}`,
    gid: raw.gid === null ? null : `${raw.gid}`,
    symlinkTarget: raw.symlinkTarget,
    fingerprint: raw.fingerprint,
    nameEncoding: raw.writableName ? "utf8" : "unsupported",
    readable: raw.kind === "file",
    writable: raw.writableName
  };
}

function fileSession(raw: WorkspaceFileSessionDto): WorkspaceFilesSession {
  return { fileSessionId: raw.fileSessionId, hostAlias: raw.hostAlias, targetKind: raw.targetKind, homePath: raw.homePath, currentPath: raw.homePath, state: "connected", reason: null };
}

function grant(raw: WorkspaceLocalPathGrantDto): WorkspaceLocalGrant {
  return { grantId: raw.grantId, displayName: raw.displayName, kind: raw.isDirectory ? "directory" : "file", expiresAt: raw.expiresAt };
}

function transferCapabilities(raw: WorkspaceTransferDto) {
  return {
    canPause: raw.state === "queued" || raw.state === "running" || raw.state === "verifying",
    canResume: raw.state === "paused" || raw.state === "interrupted",
    canCancel: !["completed", "cancelled", "finalizing"].includes(raw.state),
    canRetry: raw.state === "failed" || raw.state === "interrupted",
    canRestart: raw.state === "failed" || raw.state === "interrupted"
  };
}

/** Upload grants stay opaque; the canonical destination still carries the safe file name. */
export function transferSourceDisplayName(raw: Pick<WorkspaceTransferDto, "direction" | "sourceRef" | "destinationPath">) {
  if (raw.direction !== "upload" || !raw.sourceRef.startsWith("grant-")) return raw.sourceRef;
  const normalized = raw.destinationPath.replace(/[\\/]+$/, "");
  return normalized.split(/[\\/]/).pop() || raw.destinationPath;
}

function transfer(raw: WorkspaceTransferDto): WorkspaceTransfer {
  return {
    transferId: raw.transferId,
    revision: numberValue(raw.revision),
    createdAt: raw.createdAt,
    updatedAt: raw.updatedAt,
    direction: raw.direction,
    hostAlias: raw.hostAlias,
    sourceLabel: transferSourceDisplayName(raw),
    targetLabel: raw.destinationPath,
    state: raw.state,
    bytes: decimalText(raw.bytes),
    total: raw.total === null ? null : decimalText(raw.total),
    speedBytesPerSecond: raw.speed === null ? null : decimalText(raw.speed),
    etaSeconds: raw.etaSeconds === null ? null : numberValue(raw.etaSeconds),
    attempt: raw.attempt,
    resumable: raw.resumable,
    resumeOffset: raw.resumeOffset === null ? null : decimalText(raw.resumeOffset),
    fingerprintState: raw.fingerprintStatus === "source-verified" ? "verified" : raw.errorCode === "source-fingerprint-changed" ? "changed" : "unchecked",
    errorCode: raw.errorCode,
    errorMessage: null,
    taskId: raw.taskId,
    conflictRevision: raw.conflictRevision === null ? null : numberValue(raw.conflictRevision),
    capabilities: transferCapabilities(raw)
  };
}

function recovery(raw: WorkspaceRecoveryDto): WorkspaceRecovery {
  return {
    recoveryId: raw.recoveryId,
    hostAlias: raw.hostAlias,
    operation: raw.kind,
    originalPath: raw.originalPath,
    recoveryPath: raw.backupPath ?? "",
    createdAt: raw.createdAt,
    state: raw.state === "purge-prepared" ? "purging" : raw.state,
    taskId: raw.taskId,
    reason: raw.reason
  };
}

function localRecovery(raw: WorkspaceLocalTransferRecoveryDto): WorkspaceLocalTransferRecovery {
  return {
    recoveryId: raw.recoveryId,
    transferId: raw.transferId,
    destinationName: raw.destinationName,
    backupName: raw.backupName,
    state: raw.state,
    createdAt: raw.createdAt,
    restoredAt: raw.restoredAt,
    purgedAt: raw.purgedAt
  };
}

async function host(alias: string): Promise<HostDto> {
  const hosts = await requiredInvoke<HostDto[]>("list_hosts");
  const matched = hosts.find((item) => item.hostAlias === alias);
  if (!matched) throw new Error(`Unknown SSH host alias: ${alias}`);
  return matched;
}

function subscribe<T>(command: TauriCommand, event: string, listener: (payload: T) => void): Promise<UnlistenFn> {
  assertTauriRuntime(command);
  return listen<T>(event, ({ payload }) => listener(payload));
}

/** Adapts generated Rust DTOs to the view-specific Workspace API. */
export const desktopWorkspaceApi: WorkspaceApi = {
  listTerminalSessions: () => requiredInvoke<WorkspaceTerminalSessionDto[]>("workspace_list_terminal_sessions").then((items) => items.map(terminal)),
  openTerminal: async ({ hostAlias, columns, rows, initialDirectory }) => {
    const selected = await host(hostAlias);
    return requiredInvoke<WorkspaceTerminalSessionDto>("workspace_open_terminal", {
      request: {
        hostId: selected.id,
        hostName: selected.name,
        hostAlias,
        rows,
        cols: columns,
        autoReconnect: true,
        initialDirectory: initialDirectory ?? null
      }
    }).then(terminal);
  },
  attachTerminal: ({ sessionId, generation, afterSequence }) => requiredInvoke<WorkspaceAttachTerminalResultDto>("workspace_attach_terminal", {
    request: { sessionId, generation: generation ?? 0, afterSequence: afterSequence ?? 0 }
  }).then((raw): TerminalAttachment => ({
    session: terminal(raw.session),
    frames: raw.frames.map((frame) => ({ sessionId: raw.session.sessionId, generation: raw.session.generation, sequence: numberValue(frame.sequence), dataBase64: frame.dataBase64 })),
    oldestSequence: raw.gap ? null : null,
    latestSequence: numberValue(raw.latestSequence),
    truncated: raw.gap
  })),
  writeTerminal: ({ sessionId, generation, dataBase64 }) => requiredInvoke<void>("workspace_terminal_write", { request: { sessionId, generation, dataBase64 } }),
  resizeTerminal: ({ sessionId, generation, columns, rows }) => requiredInvoke<void>("workspace_terminal_resize", { request: { sessionId, generation, rows, cols: columns } }),
  ackTerminal: ({ sessionId, generation, sequence }) => requiredInvoke<void>("workspace_terminal_ack", { request: { sessionId, generation, sequence } }),
  reconnectTerminal: ({ sessionId }) => requiredInvoke<WorkspaceTerminalSessionDto>("workspace_reconnect_terminal", { sessionId }).then(terminal),
  closeTerminal: ({ sessionId, generation }) => requiredInvoke<void>("workspace_close_terminal", { request: { sessionId, generation } }),

  listLocalRoots: () => requiredInvoke<string[]>("workspace_list_local_roots"),
  openFiles: async ({ hostAlias }) => {
    if (!hostAlias) {
      return requiredInvoke<WorkspaceFileSessionDto>("workspace_open_files", { request: { local: true, hostId: "local", hostName: "Local files", hostAlias: "" } }).then(fileSession);
    }
    const selected = await host(hostAlias);
    return requiredInvoke<WorkspaceFileSessionDto>("workspace_open_files", { request: { local: false, hostId: selected.id, hostName: selected.name, hostAlias } }).then(fileSession);
  },
  openFolderInVscode: ({ fileSessionId, path, entryRef }) => requiredInvoke<void>("workspace_open_folder_in_vscode", {
    request: { fileSessionId, path, entryRef: entryRef ?? null }
  }),
  closeFiles: ({ fileSessionId }) => requiredInvoke<void>("workspace_close_files", { fileSessionId }),
  listDirectory: ({ fileSessionId, path, snapshotId, cursor, sort, direction }) => requiredInvoke<WorkspaceListDirectoryResultDto>("workspace_list_directory", {
    request: { fileSessionId, path: path ?? ".", snapshotId, pageToken: cursor, sort, direction }
  }).then((raw) => ({
    fileSessionId,
    canonicalPath: raw.canonicalPath,
    snapshotId: raw.snapshotId,
    entries: raw.entries.map(entry),
    nextCursor: raw.nextPageToken,
    totalKnown: raw.totalEntries,
    truncated: raw.truncated
  })),
  startFileSearch: ({ fileSessionId, path, query }) => requiredInvoke<{ searchId: string }>("workspace_start_file_search", { request: { fileSessionId, path, query } }),
  cancelFileSearch: ({ searchId }) => requiredInvoke<void>("workspace_cancel_file_search", { request: { searchId } }),
  previewFile: ({ fileSessionId, entryRef }) => requiredInvoke<WorkspaceFilePreviewDto>("workspace_preview_file", { request: { fileSessionId, entryRef } }).then((raw) => ({
    entryRef: raw.entry.entryRef, name: raw.entry.name, kind: raw.kind, mimeType: raw.mimeType, size: raw.entry.size ?? "", text: raw.text, dataBase64: raw.dataBase64, truncated: raw.truncated, blockedReason: null
  })),
  createDirectory: ({ fileSessionId, parentPath, name }) => requiredInvoke<RemoteFileEntryDto>("workspace_create_directory", {
    request: { fileSessionId, parentPath, name }
  }).then(entry),
  copyEntry: ({ fileSessionId, sourceEntryRef, destinationPath }) => requiredInvoke<RemoteFileEntryDto>("workspace_copy_file_entry", {
    request: { fileSessionId, sourceEntryRef, destinationPath }
  }).then(entry),
  copyEntries: ({ sourceFileSessionId, destinationFileSessionId, sourceEntryRefs, destinationPath }) => requiredInvoke<RemoteFileEntryDto[]>("workspace_copy_file_entries", {
    request: { sourceFileSessionId, destinationFileSessionId, sourceEntryRefs, destinationPath }
  }).then((items) => items.map(entry)),
  moveEntries: ({ sourceFileSessionId, destinationFileSessionId, sourceEntryRefs, destinationPath }) => requiredInvoke<RemoteFileEntryDto[]>("workspace_move_file_entries", {
    request: { sourceFileSessionId, destinationFileSessionId, sourceEntryRefs, destinationPath }
  }).then((items) => items.map(entry)),
  saveTextFile: ({ fileSessionId, entryRef, expectedFingerprint, text }) => requiredInvoke<RemoteFileEntryDto>("workspace_save_text_file", {
    request: { fileSessionId, entryRef, expectedFingerprint, text }
  }).then(entry),
  validateTerminalCwd: ({ sessionId, generation, fileSessionId }) => requiredInvoke<WorkspaceValidatedCwdDto>("workspace_validate_terminal_cwd", {
    request: { sessionId, generation, fileSessionId }
  }).then((raw) => ({
    sessionId: raw.sessionId,
    generation: raw.generation,
    revision: numberValue(raw.revision),
    path: raw.path,
    source: raw.source
  })),
  prepareFileOperation: async ({ fileSessionId, operation, entryRef, destinationPath }) => {
    if (!entryRef || !["delete", "rename", "move"].includes(operation)) throw new Error("This file operation requires a supported, fresh backend entry reference.");
    const kind = operation === "delete" ? "delete" : "rename";
    return requiredInvoke<WorkspacePreparedFileOperationDto>("workspace_prepare_file_operation", { request: { fileSessionId, kind, sourceEntryRef: entryRef, destinationPath, stagingEntryRef: null } }).then((raw) => ({
      token: raw.operationToken, operation: operation === "move" ? "move" : raw.kind === "rename" ? "rename" : "delete", hostAlias: raw.hostAlias, sourcePath: raw.sourcePath, targetPath: raw.destinationPath, backupPath: raw.recoveryPath, impactSummary: raw.kind, expiresAt: raw.expiresAt, requiresBackup: raw.requiresDestinationBackup
    }));
  },
  confirmFileOperation: ({ token }) => requiredInvoke<WorkspaceFileOperationResultDto>("workspace_confirm_file_operation", { request: { operationToken: token } }).then((raw) => ({ taskId: raw.taskId ?? "", recovery: recovery(raw.recovery), destinationEntry: null })),
  restoreRecovery: ({ recoveryId }) => requiredInvoke<WorkspaceFileOperationResultDto>("workspace_restore_recovery", { request: { recoveryId } }).then((raw) => ({ taskId: raw.taskId ?? "", recovery: recovery(raw.recovery), destinationEntry: null })),
  prepareRecoveryPurge: ({ recoveryId }) => requiredInvoke<WorkspacePreparedRecoveryPurgeDto>("workspace_prepare_recovery_purge", { request: { recoveryId } }).then((raw) => ({ token: raw.purgeToken, recoveryId: raw.recovery.recoveryId, hostAlias: raw.recovery.hostAlias, recoveryPath: raw.recovery.backupPath ?? "", expiresAt: raw.expiresAt })),
  purgeRecovery: ({ token }) => requiredInvoke<WorkspaceRecoveryDto>("workspace_purge_recovery", { request: { purgeToken: token } }).then(recovery),

  selectUploadSources: () => requiredInvoke<WorkspaceLocalPathGrantDto[]>("workspace_select_upload_sources").then((items) => items.map(grant)),
  selectDownloadTarget: () => requiredInvoke<WorkspaceLocalPathGrantDto | null>("workspace_select_download_target").then((item) => item ? grant(item) : null),
  enqueueTransfers: async ({ direction, hostAlias, fileSessionId, sourceEntryRefs, localGrantIds, destinationPath, conflictPolicy }) => {
    const selected = hostAlias ? await host(hostAlias) : { id: "local", name: "Local files" };
    const refs = direction === "upload" ? localGrantIds : sourceEntryRefs;
    const items = refs.map((sourceRef) => ({
      direction, hostId: selected.id, hostName: selected.name, hostAlias, sourceRef,
      localGrantId: direction === "download" ? localGrantIds[0] ?? null : null,
      destinationPath: direction === "upload" ? destinationPath ?? "" : "download",
      totalBytes: null, conflictStrategy: conflictPolicy
    }));
    return requiredInvoke<WorkspaceTransferDto[]>("workspace_enqueue_transfers", { request: { fileSessionId, items } }).then((items) => items.map(transfer));
  },
  listTransfers: () => requiredInvoke<WorkspaceTransferSnapshotDto>("workspace_list_transfers").then((raw) => ({
    transfers: raw.transfers.map(transfer),
    recoveries: raw.recoveries.map(recovery),
    localRecoveries: raw.localRecoveries.map(localRecovery)
  })),
  restoreLocalTransferRecovery: ({ recoveryId }) => requiredInvoke<WorkspaceLocalTransferRecoveryDto>("workspace_restore_local_transfer_recovery", { request: { recoveryId } }).then(localRecovery),
  prepareLocalTransferRecoveryPurge: ({ recoveryId }) => requiredInvoke<WorkspacePreparedLocalTransferRecoveryPurgeDto>("workspace_prepare_local_transfer_recovery_purge", { request: { recoveryId } }).then((raw) => ({ token: raw.purgeToken, recovery: localRecovery(raw.recovery), expiresAt: raw.expiresAt })),
  purgeLocalTransferRecovery: ({ token }) => requiredInvoke<void>("workspace_purge_local_transfer_recovery", { request: { purgeToken: token } }),
  pauseTransfer: ({ transferId, revision }) => requiredInvoke<void>("workspace_pause_transfer", { request: { transferId, revision } }),
  resumeTransfer: ({ transferId, revision, fileSessionId, localGrantId }) => requiredInvoke<void>("workspace_resume_transfer", { request: { transferId, revision, fileSessionId: fileSessionId ?? null, localGrantId: localGrantId ?? null, restart: false } }),
  cancelTransfer: ({ transferId, revision }) => requiredInvoke<void>("workspace_cancel_transfer", { request: { transferId, revision } }),
  retryTransfer: ({ transferId, revision, restart, fileSessionId, localGrantId }) => requiredInvoke<void>("workspace_retry_transfer", { request: { transferId, revision, fileSessionId: fileSessionId ?? null, localGrantId: localGrantId ?? null, restart } }),
  resolveTransferConflict: ({ transferId, conflictRevision, policy, applyToBatch }) => requiredInvoke<void>("workspace_resolve_transfer_conflict", { request: { transferId, conflictRevision, strategy: policy, applyToBatch } }),

  events: {
    onTerminalOutput: (handler) => subscribe<WorkspaceTerminalOutputEventDto>("workspace_attach_terminal", "workspace-terminal-output", (raw) => handler({ sessionId: raw.sessionId, generation: raw.generation, sequence: numberValue(raw.sequence), dataBase64: raw.dataBase64 })),
    onSessionState: (handler) => subscribe<WorkspaceSessionStateEventDto>("workspace_list_terminal_sessions", "workspace-session-state", (raw) => handler({ sessionId: raw.sessionId, generation: raw.generation, revision: numberValue(raw.revision), state: raw.state, reconnectable: raw.reconnectable, attempt: raw.attempt, nextRetryAt: raw.nextRetryAt, reason: raw.reason, taskId: raw.taskId })),
    onSessionHeartbeat: (handler) => subscribe<WorkspaceSessionHeartbeatEventDto>("workspace_list_terminal_sessions", "workspace-session-heartbeat", (raw) => handler({ sessionId: raw.sessionId, generation: raw.generation, revision: numberValue(raw.revision), receivedAt: raw.observedAt })),
    onTerminalCwd: (handler) => subscribe<WorkspaceTerminalCwdEventDto>("workspace_validate_terminal_cwd", "workspace-terminal-cwd", (raw) => handler({ sessionId: raw.sessionId, generation: raw.generation, revision: numberValue(raw.revision), path: raw.path, source: raw.source })),
    onTransferUpdated: (handler) => subscribe<WorkspaceTransferUpdatedEventDto>("workspace_list_transfers", "workspace-transfer-updated", (raw) => handler({
      transferId: raw.transferId,
      revision: numberValue(raw.revision),
      updatedAt: raw.updatedAt,
      state: raw.state,
      bytes: decimalText(raw.bytes),
      total: raw.total === null ? null : decimalText(raw.total),
      speedBytesPerSecond: raw.speed === null ? null : decimalText(raw.speed),
      etaSeconds: raw.etaSeconds === null ? null : numberValue(raw.etaSeconds),
      attempt: raw.attempt,
      resumable: raw.resumable,
      errorCode: raw.errorCode,
      taskId: raw.taskId
    })),
    onFileSearchUpdated: (handler) => subscribe<WorkspaceFileSearchUpdatedEventDto>("workspace_start_file_search", "workspace-file-search-updated", (raw) => handler({
      searchId: raw.searchId,
      fileSessionId: raw.fileSessionId,
      revision: numberValue(raw.revision),
      state: raw.state,
      entries: raw.entries.map(entry),
      scanned: raw.scanned,
      truncated: raw.truncated,
      reason: raw.errorCode
    })),
    onLocalDragState: (handler) => subscribe<{ phase: "enter" | "over" | "leave" | "drop"; clientX: number | null; clientY: number | null }>(
      "workspace_open_files",
      "workspace-local-drag-state",
      handler
    ),
    onLocalDrop: (handler) => subscribe<WorkspaceLocalPathGrantDto[]>("workspace_open_files", "workspace-local-drop", (raw) => handler({ grants: raw.map(grant) }))
  }
};

/** Mock never pretends that a PTY, remote filesystem or transfer queue exists. */
const rejectWorkspaceBackend = (): Promise<never> => Promise.reject(
  new Error("desktop-backend-required: Workspace requires the Tauri desktop backend.")
);

// `events` is a nested API object. Keep it callable at both levels so a Mock
// subscription rejects through the same explicit desktop-only contract.
const unavailableWorkspaceEvents = new Proxy({} as WorkspaceApi["events"], {
  get: () => rejectWorkspaceBackend
});

export const unavailableWorkspaceApi: WorkspaceApi = new Proxy(
  { events: unavailableWorkspaceEvents } as WorkspaceApi,
  {
    get: (target, property) => property === "events" ? target.events : rejectWorkspaceBackend
  }
);
