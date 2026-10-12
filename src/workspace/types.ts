export type WorkspaceLocale = "en" | "zh";
export type WorkspacePlatform = "windows" | "macos" | "linux";
export type WorkspaceMode = "terminal" | "files" | "split" | "transfers";

/** One-shot app-chrome request to align the selected host with a terminal PTY. */
export type WorkspaceTerminalHostRequest = {
  requestId: number;
  hostAlias: string;
};

export type WorkspaceHost = {
  id: string;
  name: string;
  hostAlias: string;
  status: "online" | "offline" | "unknown" | "testing" | string;
  latencyMs?: number | null;
};

export type TerminalSessionState =
  | "creating"
  | "connecting"
  | "connected"
  | "reconnecting"
  | "disconnected"
  | "closing"
  | "closed"
  | "failed";

export type WorkspaceTerminalSession = {
  sessionId: string;
  hostAlias: string;
  title: string;
  generation: number;
  revision: number;
  state: TerminalSessionState;
  reconnectable: boolean;
  autoReconnect: boolean;
  attempt: number;
  nextRetryAt: string | null;
  reason: string | null;
  createdAt: string;
  taskId: string | null;
};

export type TerminalOutputFrame = {
  sessionId: string;
  generation: number;
  sequence: number;
  dataBase64: string;
};

export type TerminalAttachment = {
  session: WorkspaceTerminalSession;
  frames: TerminalOutputFrame[];
  oldestSequence: number | null;
  latestSequence: number | null;
  truncated: boolean;
};

export type WorkspaceSessionStateEvent = {
  sessionId: string;
  generation: number;
  revision: number;
  state: TerminalSessionState;
  reconnectable: boolean;
  attempt: number;
  nextRetryAt: string | null;
  reason: string | null;
  taskId: string | null;
};

export type WorkspaceSessionHeartbeatEvent = {
  sessionId: string;
  generation: number;
  revision: number;
  receivedAt: string;
};

export type TerminalCwdSource = "proc" | "osc7" | "unknown";

export type WorkspaceTerminalCwdEvent = {
  sessionId: string;
  generation: number;
  revision: number;
  path: string | null;
  source: TerminalCwdSource;
};

export type WorkspaceFilesSession = {
  fileSessionId: string;
  hostAlias: string;
  targetKind: "local" | "remote";
  homePath: string;
  currentPath: string;
  state: "connecting" | "connected" | "closed" | "failed";
  reason: string | null;
};

export type RemoteFileKind = "directory" | "file" | "symlink" | "other";

export type RemoteFileEntry = {
  entryRef: string;
  canonicalPath: string;
  name: string;
  kind: RemoteFileKind;
  size: string;
  modifiedAt: string | null;
  permissions: string | null;
  uid: string | null;
  gid: string | null;
  symlinkTarget: string | null;
  fingerprint: string;
  nameEncoding: "utf8" | "unsupported";
  readable: boolean;
  writable: boolean;
};

export type WorkspaceDirectoryPage = {
  fileSessionId: string;
  canonicalPath: string;
  snapshotId: string;
  entries: RemoteFileEntry[];
  nextCursor: string | null;
  totalKnown: number | null;
  truncated: boolean;
};

export type WorkspaceFileSortField = "name" | "type" | "size" | "modified";
export type WorkspaceSortDirection = "asc" | "desc";

export type WorkspaceFileSearchEvent = {
  searchId: string;
  fileSessionId: string;
  revision: number;
  state: "running" | "completed" | "cancelled" | "failed";
  entries: RemoteFileEntry[];
  scanned: number;
  truncated: boolean;
  reason: string | null;
};

export type WorkspaceFilePreview = {
  entryRef: string;
  name: string;
  kind: "text" | "image" | "metadata";
  mimeType: string | null;
  size: string;
  text: string | null;
  dataBase64: string | null;
  truncated: boolean;
  blockedReason: string | null;
  fingerprint?: string;
};

export type WorkspaceFileClipboard = {
  mode: "copy" | "cut";
  sourceFileSessionId: string;
  sourceEntryRefs: string[];
  names: string[];
};

export type WorkspaceLocalGrant = {
  grantId: string;
  displayName: string;
  kind: "file" | "directory";
  expiresAt: string;
};

export type WorkspaceLocalDropEvent = {
  grants: WorkspaceLocalGrant[];
};

export type WorkspaceLocalDragStateEvent = {
  phase: "enter" | "over" | "leave" | "drop";
  clientX: number | null;
  clientY: number | null;
};

export type WorkspaceTransferDirection = "upload" | "download";
export type WorkspaceTransferState =
  | "queued"
  | "running"
  | "pausing"
  | "paused"
  | "waiting-conflict"
  | "verifying"
  | "finalizing"
  | "completed"
  | "failed"
  | "cancelled"
  | "interrupted";
export type WorkspaceConflictPolicy = "ask" | "skip" | "keep-both" | "replace-with-backup";

export type WorkspaceTransferCapabilities = {
  canPause: boolean;
  canResume: boolean;
  canCancel: boolean;
  canRetry: boolean;
  canRestart: boolean;
};

export type WorkspaceTransfer = {
  transferId: string;
  revision: number;
  createdAt: string;
  updatedAt: string;
  direction: WorkspaceTransferDirection;
  hostAlias: string;
  sourceLabel: string;
  targetLabel: string;
  state: WorkspaceTransferState;
  bytes: string;
  total: string | null;
  speedBytesPerSecond: string | null;
  etaSeconds: number | null;
  attempt: number;
  resumable: boolean;
  resumeOffset: string | null;
  fingerprintState: "verified" | "changed" | "unchecked";
  errorCode: string | null;
  errorMessage: string | null;
  taskId: string | null;
  conflictRevision: number | null;
  capabilities: WorkspaceTransferCapabilities;
};

/** High-frequency backend event. Immutable labels stay in the loaded snapshot. */
export type WorkspaceTransferUpdatedEvent = Pick<WorkspaceTransfer,
  "transferId" | "revision" | "updatedAt" | "state" | "bytes" | "total" |
  "speedBytesPerSecond" | "etaSeconds" | "attempt" | "resumable" |
  "errorCode" | "taskId"
>;

export type WorkspaceRecovery = {
  recoveryId: string;
  hostAlias: string;
  operation: "delete" | "overwrite" | "rename";
  originalPath: string;
  recoveryPath: string;
  createdAt: string;
  state: "prepared" | "available" | "restoring" | "restored" | "purging" | "purged" | "failed";
  taskId: string | null;
  reason: string | null;
};

export type WorkspaceTransferSnapshot = {
  transfers: WorkspaceTransfer[];
  recoveries: WorkspaceRecovery[];
  localRecoveries: WorkspaceLocalTransferRecovery[];
};

/** Desktop-only download replacement journal. Paths stay private to Rust. */
export type WorkspaceLocalTransferRecovery = {
  recoveryId: string;
  transferId: string;
  destinationName: string;
  backupName: string;
  state: "prepared" | "available" | "restored" | "purged" | "failed";
  createdAt: string;
  restoredAt: string | null;
  purgedAt: string | null;
};

export type WorkspaceLocalTransferRecoveryPurgePreview = {
  token: string;
  recovery: WorkspaceLocalTransferRecovery;
  expiresAt: string;
};

export type WorkspaceFileOperationKind = "delete" | "rename" | "move" | "copy" | "create-directory";

export type WorkspaceFileOperationPreview = {
  token: string;
  operation: WorkspaceFileOperationKind;
  hostAlias: string;
  sourcePath: string | null;
  targetPath: string | null;
  backupPath: string | null;
  impactSummary: string;
  expiresAt: string;
  requiresBackup: boolean;
};

export type WorkspaceFileOperationResult = {
  taskId: string;
  recovery: WorkspaceRecovery | null;
  destinationEntry: RemoteFileEntry | null;
};

export type WorkspaceRecoveryPurgePreview = {
  token: string;
  recoveryId: string;
  hostAlias: string;
  recoveryPath: string;
  expiresAt: string;
};

export type WorkspaceApiUnsubscribe = () => void;
export type WorkspaceApiSubscription = WorkspaceApiUnsubscribe | Promise<WorkspaceApiUnsubscribe>;

/**
 * 该接口只描述真实桌面后端能力。调用方负责把 Tauri commands/events 适配到此接口；
 * Web/mock 模式应显式拒绝，而不能填充示例会话或文件。
 */
export type WorkspaceApi = {
  listTerminalSessions: () => Promise<WorkspaceTerminalSession[]>;
  openTerminal: (input: {
    hostAlias: string;
    columns: number;
    rows: number;
    initialDirectory?: { fileSessionId: string; path: string } | null;
  }) => Promise<WorkspaceTerminalSession>;
  attachTerminal: (input: { sessionId: string; generation: number | null; afterSequence: number | null }) => Promise<TerminalAttachment>;
  writeTerminal: (input: { sessionId: string; generation: number; dataBase64: string }) => Promise<void>;
  resizeTerminal: (input: { sessionId: string; generation: number; columns: number; rows: number }) => Promise<void>;
  ackTerminal: (input: { sessionId: string; generation: number; sequence: number }) => Promise<void>;
  reconnectTerminal: (input: { sessionId: string }) => Promise<WorkspaceTerminalSession>;
  closeTerminal: (input: { sessionId: string; generation: number }) => Promise<void>;

  listLocalRoots: () => Promise<string[]>;
  openFiles: (input: { hostAlias: string }) => Promise<WorkspaceFilesSession>;
  openFolderInVscode: (input: {
    fileSessionId: string;
    path: string;
    entryRef?: string | null;
  }) => Promise<void>;
  closeFiles: (input: { fileSessionId: string }) => Promise<void>;
  listDirectory: (input: {
    fileSessionId: string;
    path: string | null;
    snapshotId: string | null;
    cursor: string | null;
    pageSize: number;
    sort: WorkspaceFileSortField;
    direction: WorkspaceSortDirection;
  }) => Promise<WorkspaceDirectoryPage>;
  startFileSearch: (input: { fileSessionId: string; path: string; query: string }) => Promise<{ searchId: string }>;
  cancelFileSearch: (input: { searchId: string }) => Promise<void>;
  previewFile: (input: { fileSessionId: string; entryRef: string }) => Promise<WorkspaceFilePreview>;
  createDirectory: (input: { fileSessionId: string; parentPath: string; name: string }) => Promise<RemoteFileEntry>;
  copyEntry: (input: { fileSessionId: string; sourceEntryRef: string; destinationPath: string }) => Promise<RemoteFileEntry>;
  copyEntries: (input: {
    sourceFileSessionId: string;
    destinationFileSessionId: string;
    sourceEntryRefs: string[];
    destinationPath: string;
  }) => Promise<RemoteFileEntry[]>;
  moveEntries: (input: {
    sourceFileSessionId: string;
    destinationFileSessionId: string;
    sourceEntryRefs: string[];
    destinationPath: string;
  }) => Promise<RemoteFileEntry[]>;
  saveTextFile: (input: {
    fileSessionId: string;
    entryRef: string;
    expectedFingerprint: string;
    text: string;
  }) => Promise<RemoteFileEntry>;
  validateTerminalCwd: (input: { sessionId: string; generation: number; fileSessionId: string }) => Promise<WorkspaceTerminalCwdEvent>;
  prepareFileOperation: (input: {
    fileSessionId: string;
    operation: WorkspaceFileOperationKind;
    entryRef: string | null;
    destinationPath: string | null;
    name: string | null;
  }) => Promise<WorkspaceFileOperationPreview>;
  confirmFileOperation: (input: { token: string }) => Promise<WorkspaceFileOperationResult>;
  restoreRecovery: (input: { recoveryId: string }) => Promise<WorkspaceFileOperationResult>;
  prepareRecoveryPurge: (input: { recoveryId: string }) => Promise<WorkspaceRecoveryPurgePreview>;
  purgeRecovery: (input: { token: string }) => Promise<WorkspaceRecovery>;
  restoreLocalTransferRecovery: (input: { recoveryId: string }) => Promise<WorkspaceLocalTransferRecovery>;
  prepareLocalTransferRecoveryPurge: (input: { recoveryId: string }) => Promise<WorkspaceLocalTransferRecoveryPurgePreview>;
  purgeLocalTransferRecovery: (input: { token: string }) => Promise<void>;

  selectUploadSources: () => Promise<WorkspaceLocalGrant[]>;
  selectDownloadTarget: () => Promise<WorkspaceLocalGrant | null>;
  enqueueTransfers: (input: {
    direction: WorkspaceTransferDirection;
    hostAlias: string;
    fileSessionId: string;
    sourceEntryRefs: string[];
    localGrantIds: string[];
    destinationPath: string | null;
    conflictPolicy: WorkspaceConflictPolicy;
  }) => Promise<WorkspaceTransfer[]>;
  listTransfers: () => Promise<WorkspaceTransferSnapshot>;
  pauseTransfer: (input: { transferId: string; revision: number }) => Promise<void>;
  resumeTransfer: (input: { transferId: string; revision: number; fileSessionId?: string; localGrantId?: string }) => Promise<void>;
  cancelTransfer: (input: { transferId: string; revision: number }) => Promise<void>;
  retryTransfer: (input: { transferId: string; revision: number; restart: boolean; fileSessionId?: string; localGrantId?: string }) => Promise<void>;
  resolveTransferConflict: (input: {
    transferId: string;
    conflictRevision: number;
    policy: Exclude<WorkspaceConflictPolicy, "ask">;
    applyToBatch: boolean;
  }) => Promise<void>;

  events: {
    onTerminalOutput: (listener: (event: TerminalOutputFrame) => void) => WorkspaceApiSubscription;
    onSessionState: (listener: (event: WorkspaceSessionStateEvent) => void) => WorkspaceApiSubscription;
    onSessionHeartbeat: (listener: (event: WorkspaceSessionHeartbeatEvent) => void) => WorkspaceApiSubscription;
    onTerminalCwd: (listener: (event: WorkspaceTerminalCwdEvent) => void) => WorkspaceApiSubscription;
    onTransferUpdated: (listener: (event: WorkspaceTransferUpdatedEvent) => void) => WorkspaceApiSubscription;
    onFileSearchUpdated: (listener: (event: WorkspaceFileSearchEvent) => void) => WorkspaceApiSubscription;
    onLocalDragState: (listener: (event: WorkspaceLocalDragStateEvent) => void) => WorkspaceApiSubscription;
    onLocalDrop: (listener: (event: WorkspaceLocalDropEvent) => void) => WorkspaceApiSubscription;
  };
};

export type WorkspaceTerminalPreferences = {
  fontFamily: "system-mono" | "cascadia" | "jetbrains" | "sf-mono";
  fontSize: number;
  lineHeight: number;
  colorScheme: "follow-app" | "light" | "dark" | "high-contrast";
  scrollback: number;
  cursorStyle: "block" | "bar" | "underline";
  screenReaderMode: boolean;
  confirmLargePaste: boolean;
};

export const defaultTerminalPreferences: WorkspaceTerminalPreferences = {
  fontFamily: "system-mono",
  fontSize: 14,
  lineHeight: 1.25,
  colorScheme: "dark",
  scrollback: 5000,
  cursorStyle: "block",
  screenReaderMode: false,
  confirmLargePaste: true
};
