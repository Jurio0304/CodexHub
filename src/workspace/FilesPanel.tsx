import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent as ReactKeyboardEvent, PointerEvent as ReactPointerEvent } from "react";
import { createPortal } from "react-dom";
import type { WorkspaceCopy } from "./copy";
import type {
  RemoteFileEntry,
  WorkspaceApi,
  WorkspaceDirectoryPage,
  WorkspaceFileOperationKind,
  WorkspaceFileOperationPreview,
  WorkspaceFileSortField,
  WorkspaceFilePreview,
  WorkspaceFileClipboard,
  WorkspaceFilesSession,
  WorkspaceHost,
  WorkspaceLocale,
  WorkspaceRecovery,
  WorkspaceTerminalCwdEvent,
  WorkspaceTerminalSession,
  WorkspaceTransfer
} from "./types";
import { workspaceHostLabel } from "./hostLabel";
import { usePersonalInfoMasking } from "../ui/PersonalInfoMasking";
import { DirectoryTree } from "./files/DirectoryTree";
import {
  FileDeleteModeDialog,
  FileDeletePreferenceDialog,
  FileEditorDialog,
  FileOperationDialog,
  FilePreviewDialog
} from "./files/FileDialogs";
import type { FileDeleteMode, PendingFileOperation } from "./files/FileDialogs";
import { FileTable } from "./files/FileTable";
import { FileTransferDrawer } from "./files/FileTransferDrawer";
import { FilesIcon } from "./files/FilesIcon";
import { childPath, parentPath } from "./files/fileDisplay";
import { filesUiCopy } from "./files/filesUiCopy";
import "./files-redesign.css";

export { formatModifiedAt } from "./files/fileDisplay";

export const WORKSPACE_FILES_LOCATION_EVENT = "codexhub:workspace-files-location";

type SortKey = WorkspaceFileSortField;

const TREE_MIN_WIDTH = 180;
const TREE_MAX_WIDTH = 520;
const FILES_TABLE_MIN_WIDTH = 360;
const DEFAULT_TREE_WIDTH = 248;
export const FILE_TRANSFER_COMPLETED_RETENTION_MS = 10_000;
export const FILE_DELETE_MODE_STORAGE_KEY = "codexhub.files-delete-mode";
export const WORKSPACE_FILES_CLIPBOARD_EVENT = "codexhub:workspace-files-clipboard";
export const WORKSPACE_FILES_REFRESH_EVENT = "codexhub:workspace-files-refresh";
const FILES_VIEW_STORAGE_PREFIX = "codexhub.workspace.files-view.v1:";
const ACTIVE_TRANSFER_STATES = new Set<WorkspaceTransfer["state"]>([
  "queued", "running", "pausing", "paused", "waiting-conflict", "verifying", "finalizing"
]);
// Files uploads behave like an explicit overwrite action while the backend
// still journals the replaced destination for recovery.
const FILE_UPLOAD_CONFLICT_POLICY = "replace-with-backup" as const;

function offsetContextMenuPoint(point: { x: number; y: number }) {
  return point;
}

function loadFileDeleteMode(): FileDeleteMode | null {
  try {
    const value = window.localStorage.getItem(FILE_DELETE_MODE_STORAGE_KEY);
    return value === "direct" || value === "backup" ? value : null;
  } catch {
    return null;
  }
}

function saveFileDeleteMode(mode: FileDeleteMode) {
  try {
    window.localStorage.setItem(FILE_DELETE_MODE_STORAGE_KEY, mode);
  } catch {
    // Hardened WebViews may disable storage; the current deletion still runs.
  }
}

type FilesHostView = {
  fileSession: WorkspaceFilesSession | null;
  page: WorkspaceDirectoryPage | null;
  filesError: boolean;
  failedPath: string | null;
  pathInput: string;
  history: string[];
  historyIndex: number;
  sortKey: SortKey;
  sortAscending: boolean;
  query: string;
  searchId: string | null;
  searchResults: RemoteFileEntry[] | null;
  searchScanned: number;
  searchTruncated: boolean;
  selectedRefs: Set<string>;
  focusedIndex: number;
  localRoots: string[];
  directoryEntriesByPath: Map<string, RemoteFileEntry[]>;
  expandedTreePaths: Set<string>;
  quickAccessPaths: string[];
  clientPage: number;
  clientPageSize: number;
};

type PersistedFilesView = {
  hostAlias: string;
  path: string | null;
  history: string[];
  historyIndex: number;
  sortKey: SortKey;
  sortAscending: boolean;
  showHidden: boolean;
  treeCollapsed: boolean;
  treeWidth: number;
  expandedTreePaths: string[];
  quickAccessPaths: string[];
  clientPageSize: number;
  followCwd: boolean;
};

function loadPersistedFilesView(instanceId: string, hostAlias: string): PersistedFilesView | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(`${FILES_VIEW_STORAGE_PREFIX}${instanceId}`);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<PersistedFilesView>;
    if (parsed.hostAlias !== hostAlias) return null;
    return {
      hostAlias,
      path: typeof parsed.path === "string" ? parsed.path : null,
      history: Array.isArray(parsed.history) ? parsed.history.filter((value): value is string => typeof value === "string").slice(-100) : [],
      historyIndex: typeof parsed.historyIndex === "number" ? parsed.historyIndex : -1,
      sortKey: parsed.sortKey === "name" || parsed.sortKey === "size" || parsed.sortKey === "modified" ? parsed.sortKey : "type",
      sortAscending: parsed.sortAscending !== false,
      showHidden: parsed.showHidden === true,
      treeCollapsed: parsed.treeCollapsed === true,
      treeWidth: typeof parsed.treeWidth === "number" ? Math.max(TREE_MIN_WIDTH, Math.min(TREE_MAX_WIDTH, parsed.treeWidth)) : DEFAULT_TREE_WIDTH,
      expandedTreePaths: Array.isArray(parsed.expandedTreePaths) ? parsed.expandedTreePaths.filter((value): value is string => typeof value === "string") : ["/"],
      quickAccessPaths: Array.isArray(parsed.quickAccessPaths)
        ? [...new Set(parsed.quickAccessPaths.filter((value): value is string => typeof value === "string" && value.length > 0))].slice(0, 50)
        : [],
      clientPageSize: parsed.clientPageSize === 25 || parsed.clientPageSize === 100 ? parsed.clientPageSize : 50,
      followCwd: parsed.followCwd !== false
    };
  } catch {
    return null;
  }
}

function savePersistedFilesView(instanceId: string, view: PersistedFilesView) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(`${FILES_VIEW_STORAGE_PREFIX}${instanceId}`, JSON.stringify(view));
  } catch {
    // Storage can be disabled by a hardened WebView; the current session remains usable.
  }
}

function pathAncestry(path: string) {
  const ancestry = new Set<string>();
  let cursor = path;
  while (true) {
    ancestry.add(cursor);
    const parent = parentPath(cursor);
    if (parent === cursor) break;
    cursor = parent;
  }
  return ancestry;
}

export function FilesPanel({
  activeTerminal,
  api,
  copy,
  cwd,
  followCwd,
  hosts,
  isActive,
  locale = "en",
  compact = false,
  selectedHostAlias,
  onError,
  onFollowCwdChange,
  onHostSelected,
  onOpenTerminalAt,
  onRecoveryCreated,
  onViewRecoveries,
  transfers = [],
  onTransfersQueued,
  instanceId = "default"
}: {
  activeTerminal: WorkspaceTerminalSession | null;
  api: WorkspaceApi;
  copy: WorkspaceCopy;
  cwd: WorkspaceTerminalCwdEvent | null;
  followCwd: boolean;
  hosts: WorkspaceHost[];
  isActive: boolean;
  locale?: WorkspaceLocale;
  /** The parent marks the split surface compact; entering it closes the tree once. */
  compact?: boolean;
  selectedHostAlias: string;
  onError: (error: unknown) => void;
  onFollowCwdChange: (follow: boolean) => void;
  onHostSelected: (hostAlias: string) => void;
  onOpenTerminalAt: (hostAlias: string, fileSessionId: string, path: string) => void;
  onRecoveryCreated: (recovery: WorkspaceRecovery) => void;
  onViewRecoveries: () => void;
  transfers?: WorkspaceTransfer[];
  onTransfersQueued?: (transfers: WorkspaceTransfer[]) => void;
  instanceId?: string;
}) {
  const ui = filesUiCopy[locale];
  const locationInputId = `workspace-files-location-${instanceId}`;
  const searchInputId = `workspace-file-search-${instanceId}`;
  // WorkspacePage supplies a stable per-window id. Keep standalone/test
  // consumers session-scoped so an unrelated panel cannot inherit another
  // panel's durable browser state.
  const persistenceEnabled = instanceId !== "default";
  const personalInfo = usePersonalInfoMasking();
  const onErrorRef = useRef(onError);
  onErrorRef.current = onError;
  const reportError = useCallback((error: unknown) => onErrorRef.current(error), []);
  const [fileSession, setFileSession] = useState<WorkspaceFilesSession | null>(null);
  const [page, setPage] = useState<WorkspaceDirectoryPage | null>(null);
  const [loading, setLoading] = useState(false);
  const [pathInput, setPathInput] = useState("");
  const [history, setHistory] = useState<string[]>([]);
  const [historyIndex, setHistoryIndex] = useState(-1);
  const [sortKey, setSortKey] = useState<SortKey>("type");
  const [sortAscending, setSortAscending] = useState(true);
  const [query, setQuery] = useState("");
  const [showHidden, setShowHidden] = useState(false);
  const [searchId, setSearchId] = useState<string | null>(null);
  const [searchResults, setSearchResults] = useState<RemoteFileEntry[] | null>(null);
  const [searchScanned, setSearchScanned] = useState(0);
  const [searchTruncated, setSearchTruncated] = useState(false);
  const [selectedRefs, setSelectedRefs] = useState<Set<string>>(() => new Set());
  const [focusedIndex, setFocusedIndex] = useState(0);
  const [localRoots, setLocalRoots] = useState<string[]>([]);
  const [directoryEntriesByPath, setDirectoryEntriesByPath] = useState<Map<string, RemoteFileEntry[]>>(() => new Map());
  const [expandedTreePaths, setExpandedTreePaths] = useState<Set<string>>(() => new Set(["/"]));
  const [quickAccessPaths, setQuickAccessPaths] = useState<string[]>([]);
  const [loadingTreePaths, setLoadingTreePaths] = useState<Set<string>>(() => new Set());
  const [clientPage, setClientPage] = useState(0);
  const [clientPageSize, setClientPageSize] = useState(50);
  const [treeCollapsed, setTreeCollapsed] = useState(() => compact);
  const [treeWidth, setTreeWidth] = useState(DEFAULT_TREE_WIDTH);
  const [treeResizing, setTreeResizing] = useState(false);
  const [preview, setPreview] = useState<WorkspaceFilePreview | null>(null);
  const [editor, setEditor] = useState<{ entry: RemoteFileEntry; text: string; original: string; busy: boolean } | null>(null);
  const [clipboard, setClipboard] = useState<WorkspaceFileClipboard | null>(null);
  const [contextMenu, setContextMenu] = useState<{
    entry: RemoteFileEntry | null;
    kind: RemoteFileEntry["kind"];
    path: string;
    source: "file-list" | "directory-tree" | "quick-access";
    x: number;
    y: number;
  } | null>(null);
  const [pendingOperation, setPendingOperation] = useState<PendingFileOperation | null>(null);
  const [operationPreview, setOperationPreview] = useState<WorkspaceFileOperationPreview | null>(null);
  const [operationBusy, setOperationBusy] = useState(false);
  const [deleteBusy, setDeleteBusy] = useState(false);
  const [defaultDeleteMode, setDefaultDeleteMode] = useState<FileDeleteMode | null>(() => loadFileDeleteMode());
  const [pendingDelete, setPendingDelete] = useState<{ entries: RemoteFileEntry[]; mode: FileDeleteMode | null } | null>(null);
  const [filesError, setFilesError] = useState(false);
  const [failedPath, setFailedPath] = useState<string | null>(null);
  const [createdRecovery, setCreatedRecovery] = useState<WorkspaceRecovery | null>(null);
  const [connectionAttempt, setConnectionAttempt] = useState(0);
  const [externalDropTarget, setExternalDropTarget] = useState<{ path: string; kind: "current" | "directory" } | null>(null);
  const [transferDrawerCollapsed, setTransferDrawerCollapsed] = useState(false);
  const [visibleTransferIds, setVisibleTransferIds] = useState<Set<string>>(() => new Set());
  const [transferDisplayNames, setTransferDisplayNames] = useState<Map<string, string>>(() => new Map());
  const locationRef = useRef<HTMLInputElement>(null);
  const moreActionsRef = useRef<HTMLDetailsElement>(null);
  const explorerRef = useRef<HTMLDivElement>(null);
  const treeResizeCleanupRef = useRef<(() => void) | null>(null);
  const requestSequence = useRef(0);
  const historyIndexRef = useRef(-1);
  const pageSortSignatureRef = useRef("");
  const sortRef = useRef({ key: sortKey, ascending: sortAscending });
  const hostViewsRef = useRef(new Map<string, FilesHostView>());
  const failedHostAliasesRef = useRef(new Set<string>());
  const failedFileSessionIdsRef = useRef(new Map<string, string>());
  const activeHostRef = useRef("");
  const compactModeRef = useRef(compact);
  const currentViewRef = useRef<FilesHostView | null>(null);
  const retryRequestRef = useRef<{ hostAlias: string; path: string | null } | null>(null);
  // undefined keeps compatibility with older runtimes that only emit Drop;
  // null means the native drop happened outside the file destination surface.
  const pendingNativeDropTargetRef = useRef<string | null | undefined>(undefined);
  const transferStatesRef = useRef(new Map<string, WorkspaceTransfer["state"]>());
  const persistenceKey = `${instanceId}:${selectedHostAlias}`;
  const [hydratedPersistenceKey, setHydratedPersistenceKey] = useState<string | null>(null);

  useEffect(() => {
    setHydratedPersistenceKey(null);
    const persisted = persistenceEnabled ? loadPersistedFilesView(instanceId, selectedHostAlias) : null;
    // Active sessions restore after the host/session reset effect below. That
    // ordering prevents restoreHostView(null) from clobbering the durable
    // preferences during a host/window switch. Inactive windows can hydrate
    // immediately because their host effect intentionally stays dormant.
    if (persisted && !isActive) {
      setPathInput(persisted.path ?? "");
      setHistory(persisted.history);
      setHistoryIndex(Math.max(-1, Math.min(persisted.history.length - 1, persisted.historyIndex)));
      setShowHidden(persisted.showHidden);
      setSortKey(persisted.sortKey);
      setSortAscending(persisted.sortAscending);
      setTreeCollapsed(compact ? true : persisted.treeCollapsed);
      setTreeWidth(persisted.treeWidth);
      setExpandedTreePaths(new Set(persisted.expandedTreePaths.length > 0 ? persisted.expandedTreePaths : ["/"]));
      setQuickAccessPaths(persisted.quickAccessPaths);
      setClientPageSize(persisted.clientPageSize);
    }
    setHydratedPersistenceKey(persistenceKey);
  }, [instanceId, isActive, onFollowCwdChange, persistenceEnabled, persistenceKey, selectedHostAlias]);

  useEffect(() => () => treeResizeCleanupRef.current?.(), []);

  useEffect(() => {
    // A user may open the tree while staying in Split, so only close it when
    // the parent actually enters compact mode instead of on every rerender.
    if (compact && !compactModeRef.current) setTreeCollapsed(true);
    compactModeRef.current = compact;
  }, [compact]);

  const treeWidthLimit = useCallback(() => {
    const availableWidth = explorerRef.current?.clientWidth;
    if (!availableWidth) return TREE_MAX_WIDTH;
    const remainingFileWidth = compact ? 72 : FILES_TABLE_MIN_WIDTH;
    return Math.max(TREE_MIN_WIDTH, Math.min(TREE_MAX_WIDTH, availableWidth - remainingFileWidth));
  }, [compact]);

  const clampTreeWidth = useCallback((width: number) => (
    Math.max(TREE_MIN_WIDTH, Math.min(treeWidthLimit(), Math.round(width)))
  ), [treeWidthLimit]);

  // The divider owns only layout state; navigating SFTP paths never resets a user-selected tree width.
  const startTreeResize = (event: ReactPointerEvent<HTMLButtonElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    treeResizeCleanupRef.current?.();

    const initialX = event.clientX;
    const initialWidth = treeWidth;
    const move = (pointerEvent: PointerEvent) => {
      setTreeWidth(clampTreeWidth(initialWidth + pointerEvent.clientX - initialX));
    };
    const stop = () => {
      window.removeEventListener("pointermove", move);
      window.removeEventListener("pointerup", stop);
      window.removeEventListener("pointercancel", stop);
      treeResizeCleanupRef.current = null;
      setTreeResizing(false);
    };

    treeResizeCleanupRef.current = stop;
    setTreeResizing(true);
    window.addEventListener("pointermove", move);
    window.addEventListener("pointerup", stop);
    window.addEventListener("pointercancel", stop);
  };

  const handleTreeResizeKeyDown = (event: ReactKeyboardEvent<HTMLButtonElement>) => {
    const step = event.shiftKey ? 48 : 16;
    const delta = event.key === "ArrowLeft" ? -step : event.key === "ArrowRight" ? step : 0;
    if (!delta) return;
    event.preventDefault();
    setTreeWidth((current) => clampTreeWidth(current + delta));
  };

  historyIndexRef.current = historyIndex;
  sortRef.current = { key: sortKey, ascending: sortAscending };
  currentViewRef.current = {
    fileSession,
    page,
    filesError,
    failedPath,
    pathInput,
    history,
    historyIndex,
    sortKey,
    sortAscending,
    query,
    searchId,
    searchResults,
    searchScanned,
    searchTruncated,
    selectedRefs,
    focusedIndex,
    localRoots,
    directoryEntriesByPath,
    expandedTreePaths,
    quickAccessPaths,
    clientPage,
    clientPageSize
  };

  useEffect(() => {
    if (!persistenceEnabled || !isActive || hydratedPersistenceKey !== persistenceKey) return;
    savePersistedFilesView(instanceId, {
      hostAlias: selectedHostAlias,
      path: page?.canonicalPath ?? (pathInput || null),
      history,
      historyIndex,
      sortKey,
      sortAscending,
      showHidden,
      treeCollapsed,
      treeWidth,
      expandedTreePaths: [...expandedTreePaths],
      quickAccessPaths,
      clientPageSize,
      followCwd
    });
  }, [clientPageSize, expandedTreePaths, followCwd, hydratedPersistenceKey, history, historyIndex, instanceId, isActive, page?.canonicalPath, pathInput, persistenceEnabled, persistenceKey, quickAccessPaths, selectedHostAlias, showHidden, sortAscending, sortKey, treeCollapsed, treeWidth]);

  const restoreHostView = useCallback((view: FilesHostView | null) => {
    setFileSession(view?.fileSession ?? null);
    setPage(view?.page ?? null);
    setFilesError(view?.filesError ?? false);
    setFailedPath(view?.failedPath ?? null);
    setPathInput(view?.pathInput ?? "");
    setHistory(view?.history ?? []);
    setHistoryIndex(view?.historyIndex ?? -1);
    setSortKey(view?.sortKey ?? "type");
    setSortAscending(view?.sortAscending ?? true);
    setQuery(view?.query ?? "");
    setSearchId(view?.searchId ?? null);
    setSearchResults(view?.searchResults ?? null);
    setSearchScanned(view?.searchScanned ?? 0);
    setSearchTruncated(view?.searchTruncated ?? false);
    setSelectedRefs(view?.selectedRefs ?? new Set());
    setFocusedIndex(view?.focusedIndex ?? 0);
    setLocalRoots(view?.localRoots ?? []);
    setDirectoryEntriesByPath(view?.directoryEntriesByPath ?? new Map());
    setExpandedTreePaths(view?.expandedTreePaths ?? new Set(["/"]));
    setQuickAccessPaths(view?.quickAccessPaths ?? []);
    setLoadingTreePaths(new Set());
    setClientPage(view?.clientPage ?? 0);
    setClientPageSize(view?.clientPageSize ?? 50);
  }, []);

  useEffect(() => {
    const receive = (event: Event) => {
      const detail = (event as CustomEvent<WorkspaceFileClipboard | null>).detail;
      setClipboard(detail ?? null);
    };
    window.addEventListener(WORKSPACE_FILES_CLIPBOARD_EVENT, receive);
    return () => window.removeEventListener(WORKSPACE_FILES_CLIPBOARD_EVENT, receive);
  }, []);

  const rememberDirectoryPage = useCallback((nextPage: WorkspaceDirectoryPage, append = false) => {
    setDirectoryEntriesByPath((current) => {
      const next = new Map(current);
      const directories = nextPage.entries.filter((entry) => entry.kind === "directory");
      const existing = append ? next.get(nextPage.canonicalPath) ?? [] : [];
      const merged = new Map(existing.map((entry) => [entry.entryRef, entry]));
      for (const entry of directories) merged.set(entry.entryRef, entry);
      next.set(nextPage.canonicalPath, [...merged.values()]);
      return next;
    });
    setExpandedTreePaths((current) => new Set([...current, ...pathAncestry(nextPage.canonicalPath)]));
  }, []);

  const navigate = useCallback(async (
    session: WorkspaceFilesSession,
    path: string | null,
    options: { manual: boolean; replaceHistory?: boolean } = { manual: true }
  ) => {
    const requestId = ++requestSequence.current;
    setLoading(true);
    try {
      const nextPage = await api.listDirectory({
        fileSessionId: session.fileSessionId,
        path,
        snapshotId: null,
        cursor: null,
        pageSize: 500,
        sort: sortRef.current.key,
        direction: sortRef.current.ascending ? "asc" : "desc"
      });
      if (requestId !== requestSequence.current) return;
      failedHostAliasesRef.current.delete(session.hostAlias);
      failedFileSessionIdsRef.current.delete(session.hostAlias);
      setFilesError(false);
      setFailedPath(null);
      pageSortSignatureRef.current = [
        session.fileSessionId,
        nextPage.canonicalPath,
        sortRef.current.key,
        sortRef.current.ascending ? "asc" : "desc"
      ].join("\u0000");
      setPage(nextPage);
      rememberDirectoryPage(nextPage);
      setPathInput(nextPage.canonicalPath);
      setSelectedRefs(new Set());
      setFocusedIndex(0);
      setClientPage(0);
      setSearchResults(null);
      setSearchId(null);
      if (options.manual) onFollowCwdChange(false);
      if (options.replaceHistory) return;
      setHistory((current) => {
        const kept = current.slice(0, historyIndexRef.current + 1);
        return [...kept, nextPage.canonicalPath];
      });
      setHistoryIndex(historyIndexRef.current + 1);
    } catch (error) {
      if (requestId === requestSequence.current) {
        if (session.targetKind === "remote") {
          // A failed remote list may have left the per-host SFTP child unusable.
          // Dispose it in order before reopening that host.
          failedHostAliasesRef.current.add(session.hostAlias);
          failedFileSessionIdsRef.current.set(session.hostAlias, session.fileSessionId);
          setFileSession(null);
          setFilesError(true);
          setFailedPath(path);
        } else if (currentViewRef.current?.page) {
          // A bad or unavailable local path does not invalidate the local file
          // authority. Keep the last usable directory available for correction.
          setPathInput(currentViewRef.current.page.canonicalPath);
          setFilesError(false);
          setFailedPath(null);
        } else {
          setFilesError(true);
          setFailedPath(path);
        }
        reportError(error);
      }
    } finally {
      if (requestId === requestSequence.current) setLoading(false);
    }
  }, [api, onFollowCwdChange, rememberDirectoryPage, reportError]);

  // Transfer completion refreshes only the directory snapshot. Keeping the
  // session, history and current page mounted prevents a disruptive blank view.
  const refreshCurrentDirectory = useCallback(async () => {
    const view = currentViewRef.current;
    if (!view?.fileSession || !view.page) return;
    const sessionId = view.fileSession.fileSessionId;
    const path = view.page.canonicalPath;
    try {
      const nextPage = await api.listDirectory({
        fileSessionId: sessionId,
        path,
        snapshotId: null,
        cursor: null,
        pageSize: 500,
        sort: sortRef.current.key,
        direction: sortRef.current.ascending ? "asc" : "desc"
      });
      const current = currentViewRef.current;
      if (current?.fileSession?.fileSessionId !== sessionId || current.page?.canonicalPath !== path) return;
      pageSortSignatureRef.current = [
        sessionId,
        nextPage.canonicalPath,
        sortRef.current.key,
        sortRef.current.ascending ? "asc" : "desc"
      ].join("\u0000");
      setPage(nextPage);
      rememberDirectoryPage(nextPage);
      const availableRefs = new Set(nextPage.entries.map((entry) => entry.entryRef));
      setSelectedRefs((selected) => new Set([...selected].filter((entryRef) => availableRefs.has(entryRef))));
    } catch (error) {
      reportError(error);
    }
  }, [api, rememberDirectoryPage, reportError]);

  useEffect(() => {
    const refreshMovedSource = (event: Event) => {
      const sourceFileSessionId = (event as CustomEvent<string>).detail;
      if (currentViewRef.current?.fileSession?.fileSessionId === sourceFileSessionId) {
        void refreshCurrentDirectory();
      }
    };
    window.addEventListener(WORKSPACE_FILES_REFRESH_EVENT, refreshMovedSource);
    return () => window.removeEventListener(WORKSPACE_FILES_REFRESH_EVENT, refreshMovedSource);
  }, [refreshCurrentDirectory]);

  useEffect(() => {
    const previousHostAlias = activeHostRef.current;
    if (
      previousHostAlias
      && previousHostAlias !== selectedHostAlias
      && currentViewRef.current
      && !failedHostAliasesRef.current.has(previousHostAlias)
    ) {
      hostViewsRef.current.set(previousHostAlias, currentViewRef.current);
    }
    activeHostRef.current = selectedHostAlias;
    requestSequence.current += 1;
    if (!isActive) {
      if (
        selectedHostAlias
        && previousHostAlias === selectedHostAlias
        && currentViewRef.current
        && !failedHostAliasesRef.current.has(selectedHostAlias)
      ) {
        hostViewsRef.current.set(selectedHostAlias, currentViewRef.current);
      }
      return;
    }
    const retryRequest = retryRequestRef.current?.hostAlias === selectedHostAlias
      ? retryRequestRef.current
      : null;
    if (retryRequest) {
      retryRequestRef.current = null;
      hostViewsRef.current.delete(selectedHostAlias);
    }
    const cachedView = hostViewsRef.current.get(selectedHostAlias);
    if (cachedView?.page && !failedHostAliasesRef.current.has(selectedHostAlias) && !retryRequest) {
      restoreHostView(cachedView);
      return;
    }
    hostViewsRef.current.delete(selectedHostAlias);
    let disposed = false;
    const persisted = persistenceEnabled ? loadPersistedFilesView(instanceId, selectedHostAlias) : null;
    setLoading(true);
    restoreHostView(null);
    if (persisted) {
      // restoreHostView(null) clears the previous host's in-memory page. Apply
      // the durable view after that reset so a saved location/preferences set
      // cannot be overwritten during the same effect pass.
      setPathInput(persisted.path ?? "");
      setHistory(persisted.history);
      setHistoryIndex(Math.max(-1, Math.min(persisted.history.length - 1, persisted.historyIndex)));
      setSortKey(persisted.sortKey);
      setSortAscending(persisted.sortAscending);
      setShowHidden(persisted.showHidden);
      setTreeCollapsed(compact ? true : persisted.treeCollapsed);
      setTreeWidth(persisted.treeWidth);
      setExpandedTreePaths(new Set(persisted.expandedTreePaths.length > 0 ? persisted.expandedTreePaths : ["/"]));
      setQuickAccessPaths(persisted.quickAccessPaths);
      setClientPageSize(persisted.clientPageSize);
      if (isActive) onFollowCwdChange(persisted.followCwd);
    }
    void (async () => {
      const staleSessionId = failedFileSessionIdsRef.current.get(selectedHostAlias);
      if (staleSessionId) {
        await api.closeFiles({ fileSessionId: staleSessionId }).catch(() => undefined);
        failedFileSessionIdsRef.current.delete(selectedHostAlias);
      }
      if (disposed) return;
      const session = await api.openFiles({ hostAlias: selectedHostAlias });
      if (disposed) return;
      const roots = session.targetKind === "local"
        ? await api.listLocalRoots().catch((error) => {
            reportError(error);
            return [session.homePath];
          })
        : [];
      if (disposed) return;
      setLocalRoots(roots);
      setFileSession(session);
      const restoredHistory = persisted?.history?.length ? persisted.history : null;
      const restoredHistoryIndex = restoredHistory
        ? Math.max(0, Math.min(restoredHistory.length - 1, persisted?.historyIndex ?? restoredHistory.length - 1))
        : -1;
      setHistory(restoredHistory ?? []);
      setHistoryIndex(restoredHistoryIndex);
      await navigate(session, persisted?.path ?? retryRequest?.path ?? session.homePath, {
        manual: false,
        replaceHistory: Boolean(restoredHistory)
      });
    })().catch((error) => {
      if (!disposed) {
        setFilesError(true);
        setFailedPath(retryRequest?.path ?? null);
        reportError(error);
      }
    }).finally(() => {
      if (!disposed) setLoading(false);
    });
    return () => {
      disposed = true;
      requestSequence.current += 1;
    };
  }, [api, connectionAttempt, instanceId, isActive, navigate, persistenceEnabled, reportError, restoreHostView, selectedHostAlias]);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void Promise.resolve(api.events.onFileSearchUpdated((event) => {
      if (event.searchId !== searchId) return;
      setSearchResults((current) => [...(current ?? []), ...event.entries]);
      setSearchScanned(event.scanned);
      setSearchTruncated(event.truncated);
      if (event.state !== "running") setSearchId(null);
      if (event.state === "failed" && event.reason) reportError(new Error(event.reason));
    })).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    }).catch(reportError);
    return () => { disposed = true; unsubscribe(); };
  }, [api, reportError, searchId]);

  const trackQueuedTransfers = useCallback((queued: WorkspaceTransfer[], displayNames: string[]) => {
    if (queued.length === 0) return;
    // Each enqueue response is one Files drawer batch. Replacing the set keeps
    // overlapping older transfers in Transfers history without leaking them
    // into the newly opened drawer.
    setVisibleTransferIds(new Set(queued.map((transfer) => transfer.transferId)));
    setTransferDisplayNames(() => {
      const next = new Map<string, string>();
      queued.forEach((transfer, index) => next.set(transfer.transferId, displayNames[index] ?? transfer.sourceLabel));
      return next;
    });
    setTransferDrawerCollapsed(false);
    onTransfersQueued?.(queued);
  }, [onTransfersQueued]);

  useEffect(() => {
    if (!isActive || !fileSession || !api.events.onLocalDragState) return;
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void Promise.resolve(api.events.onLocalDragState((event) => {
      if (disposed) return;
      if (event.phase === "leave") {
        pendingNativeDropTargetRef.current = undefined;
        setExternalDropTarget(null);
        return;
      }
      if (event.clientX === null || event.clientY === null || !page) return;
      const hit = typeof document.elementFromPoint === "function"
        ? document.elementFromPoint(event.clientX, event.clientY)
        : null;
      const directoryRow = hit?.closest<HTMLElement>("[data-workspace-drop-directory]") ?? null;
      const tablePane = hit?.closest<HTMLElement>(".workspaceFilesTablePane") ?? null;
      const target = directoryRow && explorerRef.current?.contains(directoryRow)
        ? { path: directoryRow.dataset.workspaceDropDirectory ?? page.canonicalPath, kind: "directory" as const }
        : tablePane && explorerRef.current?.contains(tablePane)
          ? { path: page.canonicalPath, kind: "current" as const }
          : null;
      setExternalDropTarget(target);
      if (event.phase === "drop") {
        pendingNativeDropTargetRef.current = target?.path ?? null;
        setExternalDropTarget(null);
      }
    })).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    }).catch(reportError);
    return () => { disposed = true; unsubscribe(); setExternalDropTarget(null); };
  }, [api, fileSession, isActive, page, reportError]);

  useEffect(() => {
    if (!isActive || !fileSession) return;
    let disposed = false;
    let unsubscribe: () => void = () => undefined;
    void Promise.resolve(api.events.onLocalDrop((event) => {
      if (disposed || !page || event.grants.length === 0) return;
      const pendingTarget = pendingNativeDropTargetRef.current;
      pendingNativeDropTargetRef.current = undefined;
      if (pendingTarget === null) return;
      const destinationPath = pendingTarget ?? page.canonicalPath;
      void api.enqueueTransfers({
        direction: "upload",
        hostAlias: selectedHostAlias,
        fileSessionId: fileSession.fileSessionId,
        sourceEntryRefs: [],
        localGrantIds: event.grants.map((grant) => grant.grantId),
        destinationPath,
        conflictPolicy: FILE_UPLOAD_CONFLICT_POLICY
      }).then((queued) => trackQueuedTransfers(queued, event.grants.map((grant) => grant.displayName))).catch(reportError);
    })).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    }).catch(reportError);
    return () => { disposed = true; unsubscribe(); };
  }, [api, fileSession, isActive, page, reportError, selectedHostAlias, trackQueuedTransfers]);

  useEffect(() => {
    if (!isActive || !followCwd || !fileSession || !activeTerminal || !cwd?.path) return;
    if (activeTerminal.hostAlias !== selectedHostAlias || cwd.source === "unknown") return;
    if (cwd.path === page?.canonicalPath) return;
    void navigate(fileSession, cwd.path, { manual: false });
  }, [activeTerminal, cwd, fileSession, followCwd, isActive, navigate, page?.canonicalPath, selectedHostAlias]);

  useEffect(() => {
    if (!isActive || !followCwd || !fileSession || !activeTerminal || activeTerminal.hostAlias !== selectedHostAlias) return;
    let disposed = false;
    const validate = () => {
      void api.validateTerminalCwd({
        sessionId: activeTerminal.sessionId,
        generation: activeTerminal.generation,
        fileSessionId: fileSession.fileSessionId
      }).catch((error) => {
        // OSC 7 is optional. An unknown cwd is a normal capability state,
        // while validation or SFTP failures remain visible to the user.
        if (!String(error).includes("terminal-cwd-unknown") && !disposed) reportError(error);
      });
    };
    validate();
    const timer = window.setInterval(validate, 2000);
    return () => {
      disposed = true;
      window.clearInterval(timer);
    };
  }, [activeTerminal, api, fileSession, followCwd, isActive, reportError, selectedHostAlias]);

  useEffect(() => {
    const focusLocation = () => {
      locationRef.current?.focus();
      locationRef.current?.select();
    };
    document.addEventListener(WORKSPACE_FILES_LOCATION_EVENT, focusLocation);
    return () => document.removeEventListener(WORKSPACE_FILES_LOCATION_EVENT, focusLocation);
  }, []);

  useEffect(() => {
    const closeMoreActions = (event: PointerEvent) => {
      const menu = moreActionsRef.current;
      const target = event.target;
      if (!menu?.open || !(target instanceof Node) || menu.contains(target)) return;
      menu.open = false;
    };
    const closeMoreActionsOnEscape = (event: KeyboardEvent) => {
      const menu = moreActionsRef.current;
      if (event.key !== "Escape" || !menu?.open) return;
      event.preventDefault();
      menu.open = false;
      menu.querySelector<HTMLElement>("summary")?.focus();
    };
    document.addEventListener("pointerdown", closeMoreActions);
    document.addEventListener("keydown", closeMoreActionsOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeMoreActions);
      document.removeEventListener("keydown", closeMoreActionsOnEscape);
    };
  }, []);

  useEffect(() => {
    if (!contextMenu) return;
    const close = () => setContextMenu(null);
    window.addEventListener("pointerdown", close);
    window.addEventListener("blur", close);
    return () => {
      window.removeEventListener("pointerdown", close);
      window.removeEventListener("blur", close);
    };
  }, [contextMenu]);

  const entries = useMemo(() => {
    const source = searchResults ?? page?.entries ?? [];
    const root = page?.canonicalPath ?? "/";
    const visible = showHidden ? source : source.filter((entry) => {
      const prefix = root === "/" ? "/" : `${root.replace(/\/+$/, "")}/`;
      const relativePath = entry.canonicalPath.startsWith(prefix)
        ? entry.canonicalPath.slice(prefix.length)
        : entry.name;
      return !relativePath.split("/").some((segment) => segment.startsWith(".") && segment.length > 1);
    });
    return searchResults === null && query
      ? visible.filter((entry) => entry.name.toLocaleLowerCase().includes(query.toLocaleLowerCase()))
      : visible;
  }, [page?.canonicalPath, page?.entries, query, searchResults, showHidden]);

  const selectedEntries = entries.filter((entry) => selectedRefs.has(entry.entryRef));
  const clientPageCount = Math.max(1, Math.ceil(entries.length / clientPageSize));
  const paginatedEntries = useMemo(
    () => entries.slice(clientPage * clientPageSize, (clientPage + 1) * clientPageSize),
    [clientPage, clientPageSize, entries]
  );
  const visibleTreeDirectories = useMemo(() => {
    if (showHidden) return directoryEntriesByPath;
    const next = new Map<string, RemoteFileEntry[]>();
    for (const [path, directoryEntries] of directoryEntriesByPath) {
      next.set(path, directoryEntries.filter((entry) => !entry.name.startsWith(".") || entry.name === "." || entry.name === ".."));
    }
    return next;
  }, [directoryEntriesByPath, showHidden]);
  const treeEntryByPath = useMemo(() => {
    const entriesByPath = new Map<string, RemoteFileEntry>();
    for (const directoryEntries of directoryEntriesByPath.values()) {
      for (const entry of directoryEntries) entriesByPath.set(entry.canonicalPath, entry);
    }
    return entriesByPath;
  }, [directoryEntriesByPath]);
  const drawerTransfers = useMemo(() => transfers
    .filter((transfer) => transfer.hostAlias === selectedHostAlias)
    .filter((transfer) => visibleTransferIds.has(transfer.transferId))
    .sort((left, right) => right.updatedAt.localeCompare(left.updatedAt)), [selectedHostAlias, transfers, visibleTransferIds]);
  const drawerBatchFinished = drawerTransfers.length > 0
    && drawerTransfers.every((transfer) => !ACTIVE_TRANSFER_STATES.has(transfer.state));
  const clearVisibleTransferBatch = useCallback(() => {
    setVisibleTransferIds(new Set());
    setTransferDisplayNames(new Map());
  }, []);

  useEffect(() => {
    if (!drawerBatchFinished) return;
    if (transferDrawerCollapsed) {
      clearVisibleTransferBatch();
      return;
    }
    const timer = window.setTimeout(() => {
      setTransferDrawerCollapsed(true);
      clearVisibleTransferBatch();
    }, FILE_TRANSFER_COMPLETED_RETENTION_MS);
    return () => window.clearTimeout(timer);
  }, [clearVisibleTransferBatch, drawerBatchFinished, transferDrawerCollapsed]);

  const toggleTransferDrawer = () => {
    if (!transferDrawerCollapsed && drawerBatchFinished) clearVisibleTransferBatch();
    setTransferDrawerCollapsed((value) => !value);
  };

  useEffect(() => {
    let refreshNeeded = false;
    for (const transfer of transfers) {
      const previous = transferStatesRef.current.get(transfer.transferId);
      transferStatesRef.current.set(transfer.transferId, transfer.state);
      if (
        visibleTransferIds.has(transfer.transferId)
        && transfer.direction === "upload"
        && transfer.state === "completed"
        && previous !== "completed"
        && parentPath(transfer.targetLabel) === page?.canonicalPath
      ) refreshNeeded = true;
    }
    if (refreshNeeded) void refreshCurrentDirectory();
  }, [page?.canonicalPath, refreshCurrentDirectory, transfers, visibleTransferIds]);

  useEffect(() => {
    setClientPage((current) => Math.min(current, clientPageCount - 1));
  }, [clientPageCount]);

  useEffect(() => {
    setClientPage(0);
    setFocusedIndex(0);
  }, [clientPageSize, page?.canonicalPath, query, showHidden]);

  const toggleTreePath = async (path: string) => {
    const expanded = expandedTreePaths.has(path);
    setExpandedTreePaths((current) => {
      const next = new Set(current);
      if (expanded) next.delete(path); else next.add(path);
      return next;
    });
    if (expanded || !fileSession || directoryEntriesByPath.has(path)) return;
    setLoadingTreePaths((current) => new Set(current).add(path));
    try {
      const treePage = await api.listDirectory({
        fileSessionId: fileSession.fileSessionId,
        path,
        snapshotId: null,
        cursor: null,
        pageSize: 500,
        sort: "name",
        direction: "asc"
      });
      rememberDirectoryPage(treePage);
    } catch (error) {
      reportError(error);
    } finally {
      setLoadingTreePaths((current) => {
        const next = new Set(current);
        next.delete(path);
        return next;
      });
    }
  };

  const selectEntry = (entry: RemoteFileEntry, toggle: boolean) => {
    setSelectedRefs((current) => {
      if (!toggle) return new Set([entry.entryRef]);
      const next = new Set(current);
      if (next.has(entry.entryRef)) next.delete(entry.entryRef); else next.add(entry.entryRef);
      return next;
    });
  };

  const selectPage = (selected: boolean) => {
    setSelectedRefs((current) => {
      const next = new Set(current);
      for (const entry of paginatedEntries) {
        if (selected) next.add(entry.entryRef); else next.delete(entry.entryRef);
      }
      return next;
    });
  };

  const changeSort = (nextKey: WorkspaceFileSortField) => {
    // A newly selected column starts ascending; clicking the active column
    // again reverses its direction.
    setSortAscending((current) => sortKey === nextKey ? !current : true);
    setSortKey(nextKey);
  };

  const loadMore = async () => {
    if (!fileSession || !page?.nextCursor) return;
    setLoading(true);
    try {
      const next = await api.listDirectory({
        fileSessionId: fileSession.fileSessionId,
        path: page.canonicalPath,
        snapshotId: page.snapshotId,
        cursor: page.nextCursor,
        pageSize: 500,
        sort: sortKey,
        direction: sortAscending ? "asc" : "desc"
      });
      setPage((current) => current && current.snapshotId === next.snapshotId
        ? { ...next, entries: [...current.entries, ...next.entries] }
        : next);
      rememberDirectoryPage(next, true);
    } catch (error) {
      reportError(error);
    } finally {
      setLoading(false);
    }
  };

  useEffect(() => {
    if (!fileSession || !page) return;
    const signature = [
      fileSession.fileSessionId,
      page.canonicalPath,
      sortKey,
      sortAscending ? "asc" : "desc"
    ].join("\u0000");
    if (pageSortSignatureRef.current === signature) return;
    void navigate(fileSession, page.canonicalPath, { manual: false, replaceHistory: true });
  }, [fileSession, navigate, page?.canonicalPath, sortAscending, sortKey]);

  const startSearch = async () => {
    if (!fileSession || !page || !query.trim()) return;
    if (searchId) {
      await api.cancelFileSearch({ searchId }).catch(reportError);
      setSearchId(null);
      return;
    }
    setSearchResults([]);
    setSearchScanned(0);
    setSearchTruncated(false);
    try {
      const result = await api.startFileSearch({
        fileSessionId: fileSession.fileSessionId,
        path: page.canonicalPath,
        query: query.trim()
      });
      setSearchId(result.searchId);
    } catch (error) {
      setSearchResults(null);
      reportError(error);
    }
  };

  const showPreview = async (entry: RemoteFileEntry) => {
    if (!fileSession) return;
    try {
      setPreview(await api.previewFile({ fileSessionId: fileSession.fileSessionId, entryRef: entry.entryRef }));
    } catch (error) {
      reportError(error);
    }
  };

  const deleteEntries = async (targets: RemoteFileEntry[], mode: FileDeleteMode) => {
    if (!fileSession || targets.length === 0) return;
    setDeleteBusy(true);
    let latestRecovery: WorkspaceRecovery | null = null;
    try {
      for (const entry of targets) {
        const prepared = await api.prepareFileOperation({
          fileSessionId: fileSession.fileSessionId,
          operation: "delete",
          entryRef: entry.entryRef,
          destinationPath: null,
          name: null
        });
        const result = await api.confirmFileOperation({ token: prepared.token });
        if (!result.recovery) throw new Error("Delete completed without a recovery record.");
        if (mode === "direct") {
          try {
            const purge = await api.prepareRecoveryPurge({ recoveryId: result.recovery.recoveryId });
            await api.purgeRecovery({ token: purge.token });
          } catch (error) {
            // If permanent cleanup fails, expose the retained recovery instead
            // of hiding a still-restorable payload from the user.
            onRecoveryCreated(result.recovery);
            setCreatedRecovery(result.recovery);
            throw error;
          }
        } else {
          latestRecovery = result.recovery;
          onRecoveryCreated(result.recovery);
        }
      }
      if (latestRecovery) setCreatedRecovery(latestRecovery);
      setSelectedRefs(new Set());
    } catch (error) {
      reportError(error);
    } finally {
      await refreshCurrentDirectory();
      setDeleteBusy(false);
    }
  };

  const requestDelete = (targets: RemoteFileEntry[]) => {
    const deletable = targets.filter((entry) => entry.writable && entry.nameEncoding === "utf8");
    if (deletable.length === 0 || deleteBusy) return;
    if (moreActionsRef.current) moreActionsRef.current.open = false;
    if (defaultDeleteMode) {
      void deleteEntries(deletable, defaultDeleteMode);
      return;
    }
    setPendingDelete({ entries: deletable, mode: null });
  };

  const confirmDeletePreference = (remember: boolean) => {
    if (!pendingDelete?.mode) return;
    const { entries: targets, mode } = pendingDelete;
    if (remember) {
      saveFileDeleteMode(mode);
      setDefaultDeleteMode(mode);
    }
    setPendingDelete(null);
    void deleteEntries(targets, mode);
  };

  const askOperation = (operation: WorkspaceFileOperationKind, entry: RemoteFileEntry | null, destinationPath: string | null = null) => {
    if (entry?.nameEncoding === "unsupported") return;
    if (operation === "move") {
      const pending = { operation, entry, name: "", destinationPath };
      setPendingOperation(pending);
      void prepareOperation(pending);
      return;
    }
    setPendingOperation({ operation, entry, name: operation === "rename" ? entry?.name ?? "" : operation === "copy" ? `${entry?.name ?? ""} copy` : "", destinationPath });
  };

  const prepareOperation = async (pending = pendingOperation) => {
    if (!fileSession || !pending) return;
    const name = pending.name.trim();
    if (["rename", "copy", "create-directory"].includes(pending.operation) && (!name || name.includes("/") || name.includes("\\"))) return;
    setOperationBusy(true);
    try {
      if (pending.operation === "create-directory") {
        await api.createDirectory({
          fileSessionId: fileSession.fileSessionId,
          parentPath: pending.destinationPath ?? page?.canonicalPath ?? fileSession.homePath,
          name
        });
        setPendingOperation(null);
        setOperationPreview(null);
        await navigate(fileSession, page?.canonicalPath ?? null, { manual: false, replaceHistory: true });
        return;
      }
      if (pending.operation === "copy" && pending.entry) {
        await api.copyEntry({
          fileSessionId: fileSession.fileSessionId,
          sourceEntryRef: pending.entry.entryRef,
          destinationPath: childPath(parentPath(pending.entry.canonicalPath), name)
        });
        setPendingOperation(null);
        setOperationPreview(null);
        await navigate(fileSession, page?.canonicalPath ?? null, { manual: false, replaceHistory: true });
        return;
      }
      const destinationPath = pending.operation === "rename" && pending.entry
        ? childPath(parentPath(pending.entry.canonicalPath), name)
        : pending.operation === "move" && pending.entry && pending.destinationPath
          ? childPath(pending.destinationPath, pending.entry.name)
          : pending.destinationPath;
      const prepared = await api.prepareFileOperation({
        fileSessionId: fileSession.fileSessionId,
        operation: pending.operation,
        entryRef: pending.entry?.entryRef ?? null,
        destinationPath,
        name: null
      });
      setOperationPreview(prepared);
    } catch (error) {
      reportError(error);
    } finally {
      setOperationBusy(false);
    }
  };

  const confirmOperation = async () => {
    if (!operationPreview || !fileSession) return;
    setOperationBusy(true);
    try {
      const result = await api.confirmFileOperation({ token: operationPreview.token });
      if (result.recovery) {
        onRecoveryCreated(result.recovery);
        setCreatedRecovery(result.recovery);
      }
      setOperationPreview(null);
      setPendingOperation(null);
      await navigate(fileSession, page?.canonicalPath ?? null, { manual: false, replaceHistory: true });
    } catch (error) {
      reportError(error);
    } finally {
      setOperationBusy(false);
    }
  };

  const upload = async (destinationPath = page?.canonicalPath ?? null) => {
    if (!fileSession || !destinationPath) return;
    try {
      const grants = await api.selectUploadSources();
      if (grants.length === 0) return;
      const queued = await api.enqueueTransfers({
        direction: "upload",
        hostAlias: selectedHostAlias,
        fileSessionId: fileSession.fileSessionId,
        sourceEntryRefs: [],
        localGrantIds: grants.map((grant) => grant.grantId),
        destinationPath,
        conflictPolicy: FILE_UPLOAD_CONFLICT_POLICY
      });
      trackQueuedTransfers(queued, grants.map((grant) => grant.displayName));
    } catch (error) {
      reportError(error);
    }
  };

  const download = async (targets = selectedEntries) => {
    if (!fileSession || targets.length === 0) return;
    try {
      const grant = await api.selectDownloadTarget();
      if (!grant) return;
      const queued = await api.enqueueTransfers({
        direction: "download",
        hostAlias: selectedHostAlias,
        fileSessionId: fileSession.fileSessionId,
        sourceEntryRefs: targets.map((entry) => entry.entryRef),
        localGrantIds: [grant.grantId],
        destinationPath: null,
        conflictPolicy: "ask"
      });
      trackQueuedTransfers(queued, targets.map((entry) => entry.name));
    } catch (error) {
      reportError(error);
    }
  };

  const openEntry = (entry: RemoteFileEntry) => {
    if (!fileSession) return;
    if (entry.kind === "directory") void navigate(fileSession, entry.canonicalPath, { manual: true });
    else void showPreview(entry);
  };

  const editEntry = async (entry: RemoteFileEntry) => {
    if (!fileSession || entry.kind !== "file") return;
    try {
      const next = await api.previewFile({ fileSessionId: fileSession.fileSessionId, entryRef: entry.entryRef });
      if (next.kind !== "text" || next.text === null) throw new Error(copy.noPreview);
      setEditor({ entry, text: next.text, original: next.text, busy: false });
    } catch (error) {
      reportError(error);
    }
  };

  const saveEditor = async () => {
    if (!editor || !fileSession || editor.text === editor.original) return;
    setEditor((current) => current ? { ...current, busy: true } : current);
    try {
      await api.saveTextFile({
        fileSessionId: fileSession.fileSessionId,
        entryRef: editor.entry.entryRef,
        expectedFingerprint: editor.entry.fingerprint,
        text: editor.text
      });
      setEditor(null);
      await refreshCurrentDirectory();
    } catch (error) {
      setEditor((current) => current ? { ...current, busy: false } : current);
      reportError(error);
    }
  };

  const storeEntriesInClipboard = (mode: WorkspaceFileClipboard["mode"], targets: RemoteFileEntry[]) => {
    if (!fileSession || targets.length === 0) return;
    const next: WorkspaceFileClipboard = {
      mode,
      sourceFileSessionId: fileSession.fileSessionId,
      sourceEntryRefs: targets.map((entry) => entry.entryRef),
      names: targets.map((entry) => entry.name)
    };
    setClipboard(next);
    window.dispatchEvent(new CustomEvent<WorkspaceFileClipboard>(WORKSPACE_FILES_CLIPBOARD_EVENT, { detail: next }));
  };

  const copyEntriesToClipboard = (targets: RemoteFileEntry[]) => storeEntriesInClipboard("copy", targets);
  const cutEntriesToClipboard = (targets: RemoteFileEntry[]) => storeEntriesInClipboard("cut", targets);

  const pasteClipboard = async () => {
    if (!clipboard || !fileSession || !page) return;
    try {
      const paste = clipboard.mode === "cut" ? api.moveEntries : api.copyEntries;
      await paste({
        sourceFileSessionId: clipboard.sourceFileSessionId,
        destinationFileSessionId: fileSession.fileSessionId,
        sourceEntryRefs: clipboard.sourceEntryRefs,
        destinationPath: page.canonicalPath
      });
      if (clipboard.mode === "cut") {
        if (clipboard.sourceFileSessionId !== fileSession.fileSessionId) {
          window.dispatchEvent(new CustomEvent<string>(WORKSPACE_FILES_REFRESH_EVENT, {
            detail: clipboard.sourceFileSessionId
          }));
        }
        setClipboard(null);
        window.dispatchEvent(new CustomEvent<WorkspaceFileClipboard | null>(WORKSPACE_FILES_CLIPBOARD_EVENT, {
          detail: null
        }));
      }
      await refreshCurrentDirectory();
    } catch (error) {
      reportError(error);
    }
  };

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (!isActive) return;
      const target = event.target;
      if (target instanceof HTMLElement && (target.isContentEditable || ["INPUT", "TEXTAREA", "SELECT"].includes(target.tagName))) return;
      const primary = event.ctrlKey || event.metaKey;
      if (!primary || event.altKey) return;
      if (event.key.toLowerCase() === "c" && selectedEntries.length > 0) {
        event.preventDefault();
        copyEntriesToClipboard(selectedEntries);
      } else if (event.key.toLowerCase() === "x" && selectedEntries.length > 0) {
        event.preventDefault();
        cutEntriesToClipboard(selectedEntries);
      } else if (event.key.toLowerCase() === "v" && clipboard) {
        event.preventDefault();
        void pasteClipboard();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [clipboard, isActive, pasteClipboard, selectedEntries]);

  const openFolderInVscode = (entry: RemoteFileEntry | null = null, pathOverride: string | null = null) => {
    if (!fileSession) return;
    const path = entry?.kind === "directory" ? entry.canonicalPath : pathOverride ?? page?.canonicalPath;
    if (!path) return;
    void api.openFolderInVscode({
      fileSessionId: fileSession.fileSessionId,
      path,
      entryRef: entry?.kind === "directory" ? entry.entryRef : null
    }).catch(reportError);
  };

  const locateTerminal = async () => {
    if (!fileSession || !activeTerminal || activeTerminal.hostAlias !== selectedHostAlias) return;
    try {
      const verified = await api.validateTerminalCwd({
        sessionId: activeTerminal.sessionId,
        generation: activeTerminal.generation,
        fileSessionId: fileSession.fileSessionId
      });
      if (!verified.path || verified.source === "unknown") throw new Error(copy.cwdUnavailable);
      onFollowCwdChange(true);
      await navigate(fileSession, verified.path, { manual: false });
    } catch (error) {
      reportError(error);
    }
  };

  const canLocate = Boolean(fileSession && activeTerminal && activeTerminal.hostAlias === selectedHostAlias);
  const addQuickAccess = (path: string) => {
    setQuickAccessPaths((current) => current.includes(path) ? current : [...current, path].slice(-50));
    setContextMenu(null);
  };
  const removeQuickAccess = (path: string) => {
    setQuickAccessPaths((current) => current.filter((candidate) => candidate !== path));
    setContextMenu(null);
  };
  const contextTargets = contextMenu?.entry
    ? selectedRefs.has(contextMenu.entry.entryRef)
      ? selectedEntries
      : [contextMenu.entry]
    : [];
  const cutRefs = clipboard?.mode === "cut" && clipboard.sourceFileSessionId === fileSession?.fileSessionId
    ? new Set(clipboard.sourceEntryRefs)
    : new Set<string>();
  const pauseTransfer = async (transfer: WorkspaceTransfer) => {
    try {
      await api.pauseTransfer({ transferId: transfer.transferId, revision: transfer.revision });
    } catch (error) {
      reportError(error);
    }
  };
  const resumeTransfer = async (transfer: WorkspaceTransfer) => {
    try {
      await api.resumeTransfer({
        transferId: transfer.transferId,
        revision: transfer.revision,
        fileSessionId: fileSession?.fileSessionId
      });
    } catch (error) {
      reportError(error);
    }
  };
  const cancelTransfer = async (transfer: WorkspaceTransfer) => {
    try {
      await api.cancelTransfer({ transferId: transfer.transferId, revision: transfer.revision });
    } catch (error) {
      reportError(error);
    }
  };
  const operationNameValid = pendingOperation
    ? !["rename", "copy", "create-directory"].includes(pendingOperation.operation)
      || Boolean(pendingOperation.name.trim() && !/[\\/]/.test(pendingOperation.name))
    : false;

  return (
    <section
      className="workspaceFilesPanel workspaceFilesRedesign"
      aria-label={copy.modes.files}
      data-compact={compact}
      data-tree-collapsed={treeCollapsed}
      data-tree-resizing={treeResizing}
    >
      <div className="workspacePaneToolbar workspaceFilesCommandBar">
        <label className="workspaceFilesHostPicker" data-connected={fileSession?.state === "connected"}>
          {fileSession?.state === "connected" ? (
            <span aria-label={fileSession.targetKind === "local" ? ui.localConnected : ui.sftpConnected} className="workspaceFilesHostStatus" title={fileSession.targetKind === "local" ? ui.localConnected : ui.sftpConnected}>
              <i aria-hidden="true" />
            </span>
          ) : null}
          <select aria-label={copy.host} value={selectedHostAlias} onChange={(event) => onHostSelected(event.target.value)}>
            <option value="">{copy.localFiles}</option>
            {hosts.map((host) => <option key={host.id} value={host.hostAlias}>{personalInfo.maskText(workspaceHostLabel(host))}</option>)}
          </select>
        </label>
        <div aria-label={copy.location} className="workspaceButtonGroup workspaceFilesPathActions" role="group">
          <button aria-label={copy.back} disabled={!fileSession || historyIndex <= 0} title={copy.back} type="button" onClick={() => {
            if (!fileSession || historyIndex <= 0) return;
            const nextIndex = historyIndex - 1;
            setHistoryIndex(nextIndex);
            void navigate(fileSession, history[nextIndex], { manual: true, replaceHistory: true });
          }}><FilesIcon name="back" /></button>
          <button aria-label={copy.forward} disabled={!fileSession || historyIndex >= history.length - 1} title={copy.forward} type="button" onClick={() => {
            if (!fileSession || historyIndex >= history.length - 1) return;
            const nextIndex = historyIndex + 1;
            setHistoryIndex(nextIndex);
            void navigate(fileSession, history[nextIndex], { manual: true, replaceHistory: true });
          }}><FilesIcon name="forward" /></button>
          <button aria-label={copy.up} disabled={!fileSession || !page || parentPath(page.canonicalPath) === page.canonicalPath} title={copy.up} type="button" onClick={() => {
            if (fileSession && page) void navigate(fileSession, parentPath(page.canonicalPath), { manual: true });
          }}><FilesIcon name="up" /></button>
          <button aria-label={copy.home} disabled={!fileSession} title={copy.home} type="button" onClick={() => {
            if (fileSession) void navigate(fileSession, fileSession.homePath, { manual: true });
          }}><FilesIcon name="home" /></button>
          <button aria-label={copy.refresh} disabled={!fileSession} title={copy.refresh} type="button" onClick={() => {
            if (fileSession) void navigate(fileSession, page?.canonicalPath ?? null, { manual: false, replaceHistory: true });
          }}><FilesIcon name="refresh" /></button>
        </div>
        <form className="workspaceLocationForm" onSubmit={(event) => {
          event.preventDefault();
          if (fileSession && pathInput.trim()) void navigate(fileSession, pathInput.trim(), { manual: true });
        }}>
           <label className="workspaceVisuallyHidden" htmlFor={locationInputId}>{copy.location}</label>
           <input ref={locationRef} id={locationInputId} title={pathInput} value={pathInput} onChange={(event) => setPathInput(event.target.value)} />
        </form>
        <form className="workspaceSearchForm" onSubmit={(event) => { event.preventDefault(); void startSearch(); }}>
           <label className="workspaceVisuallyHidden" htmlFor={searchInputId}>{copy.searchFiles}</label>
           <input id={searchInputId} placeholder={copy.searchFiles} title={copy.recursiveSearch} value={query} onChange={(event) => {
            setQuery(event.target.value);
            if (!event.target.value) setSearchResults(null);
          }} />
          <button aria-label={searchId ? copy.stopSearch : copy.searchFiles} type="submit" disabled={!query.trim()}>
            <FilesIcon name={searchId ? "close" : "search"} />
          </button>
        </form>
        <button aria-label={copy.upload} className="workspaceFilesUploadButton" disabled={!fileSession} title={copy.upload} type="button" onClick={() => void upload()}><FilesIcon name="upload" /> <span>{copy.upload}</span></button>
        <details className="workspaceFilesMoreActions" ref={moreActionsRef}>
          <summary aria-label={ui.moreActions} title={ui.moreActions}><FilesIcon name="more" /></summary>
          <div aria-label={ui.moreActions} className="workspaceFilesMoreMenu" role="group">
          <button
            aria-label={treeCollapsed ? ui.showTree : ui.hideTree}
            aria-pressed={!treeCollapsed}
            title={treeCollapsed ? ui.showTree : ui.hideTree}
            type="button"
            onClick={() => setTreeCollapsed((value) => !value)}
          ><FilesIcon name="tree" /> <span>{ui.directoryTree}</span></button>
          <button
            aria-label={copy.followCwd}
            aria-pressed={followCwd}
            className="workspaceToggleButton"
            disabled={!canLocate}
            title={!canLocate ? copy.cwdUnavailable : copy.followCwd}
            type="button"
            onClick={() => { if (followCwd) onFollowCwdChange(false); else void locateTerminal(); }}
          ><FilesIcon name="locate" /> <span>{copy.followCwd}</span></button>
          <button
            aria-label={showHidden ? copy.hideHidden : copy.showHidden}
            aria-pressed={showHidden}
            className="workspaceToggleButton"
            title={showHidden ? copy.hideHidden : copy.showHidden}
            type="button"
            onClick={() => setShowHidden((value) => !value)}
          ><FilesIcon name={showHidden ? "eyeOff" : "eye"} /> <span>{showHidden ? copy.hideHidden : copy.showHidden}</span></button>
          <label className="workspaceFilesMoreSelect">
            <span>{copy.sortBy}</span>
            <select value={sortKey} onChange={(event) => changeSort(event.target.value as SortKey)}>
              <option value="name">{copy.sortName}</option>
              <option value="type">{copy.sortType}</option>
              <option value="size">{copy.sortSize}</option>
              <option value="modified">{copy.sortModified}</option>
            </select>
          </label>
          <button aria-label={sortAscending ? copy.ascending : copy.descending} title={sortAscending ? copy.ascending : copy.descending} type="button" onClick={() => setSortAscending((value) => !value)}><FilesIcon name={sortAscending ? "sortAscending" : "sortDescending"} /><span>{sortAscending ? copy.ascending : copy.descending}</span></button>
           <button aria-label={copy.download} disabled={selectedEntries.length === 0} title={copy.download} type="button" onClick={() => void download()}><FilesIcon name="download" /><span>{copy.download}</span></button>
           <button aria-label={copy.copyEntry} disabled={selectedEntries.length === 0} title={copy.copyEntry} type="button" onClick={() => copyEntriesToClipboard(selectedEntries)}><FilesIcon name="copy" /><span>{copy.copyEntry}</span></button>
           <button aria-label={copy.cutEntry} disabled={selectedEntries.length === 0} title={copy.cutEntry} type="button" onClick={() => cutEntriesToClipboard(selectedEntries)}><FilesIcon name="cut" /><span>{copy.cutEntry}</span></button>
           <button aria-label={copy.pasteFiles} disabled={!clipboard || !fileSession || !page} title={!clipboard ? copy.clipboardEmpty : copy.pasteFiles} type="button" onClick={() => void pasteClipboard()}><FilesIcon name="paste" /><span>{copy.pasteFiles}</span></button>
          <button aria-label={copy.openFolderInVscode} disabled={!fileSession || !page} title={!fileSession || !page ? copy.openFolderInVscodeUnavailable : copy.openFolderInVscode} type="button" onClick={() => openFolderInVscode(null, page?.canonicalPath ?? null)}><FilesIcon name="folderPlus" /><span>{copy.openFolderInVscode}</span></button>
          <button aria-label={copy.newFolder} disabled={!fileSession || !page} title={copy.newFolder} type="button" onClick={() => askOperation("create-directory", null, page?.canonicalPath ?? null)}><FilesIcon name="folderPlus" /><span>{copy.newFolder}</span></button>
          <button aria-label={copy.delete} className="workspaceFilesDeleteButton" disabled={selectedEntries.length === 0 || deleteBusy} title={copy.delete} type="button" onClick={() => requestDelete(selectedEntries)}><FilesIcon name="trash" /><span>{copy.delete}</span></button>
          </div>
        </details>
      </div>

      {!followCwd && activeTerminal?.hostAlias === selectedHostAlias ? <div className="workspaceInfoBar">{copy.followCwdPaused}</div> : null}
      {searchResults !== null ? <div className="workspaceSearchSummary">{searchScanned} {copy.scanned}{searchTruncated ? ` · ${copy.resultsTruncated}` : ""}</div> : null}
      {createdRecovery ? (
        <div className="workspaceRecoveryNotice" role="status">
          <span>{copy.recoveryReady}</span>
          <button type="button" onClick={onViewRecoveries}>{copy.viewRecovery}</button>
          <button aria-label={copy.close} type="button" onClick={() => setCreatedRecovery(null)}><FilesIcon name="close" /></button>
        </div>
      ) : null}

      <div
        className="workspaceFilesExplorer"
        ref={explorerRef}
        style={{ "--workspace-files-tree-width": `${treeWidth}px` } as CSSProperties}
      >
        {!treeCollapsed ? (
          <>
            <DirectoryTree
              copy={ui}
              currentPath={page?.canonicalPath ?? null}
              directoryEntriesByPath={visibleTreeDirectories}
              expandedPaths={expandedTreePaths}
              localRoots={localRoots}
              loadingPaths={loadingTreePaths}
              quickAccessPaths={quickAccessPaths}
              session={fileSession}
              onContextMenu={(path, point) => {
                setContextMenu({ entry: treeEntryByPath.get(path) ?? null, kind: "directory", path, source: "directory-tree", ...offsetContextMenuPoint(point) });
              }}
              onNavigate={(path) => { if (fileSession) void navigate(fileSession, path, { manual: true }); }}
              onQuickAccessContextMenu={(path, point) => {
                setContextMenu({ entry: null, kind: "directory", path, source: "quick-access", ...offsetContextMenuPoint(point) });
              }}
              onToggle={(path) => void toggleTreePath(path)}
            />
            <button
              aria-label={ui.resizeDirectoryTree}
              aria-orientation="vertical"
              aria-valuemax={TREE_MAX_WIDTH}
              aria-valuemin={TREE_MIN_WIDTH}
              aria-valuenow={treeWidth}
              className="workspaceFilesTreeResizeHandle"
              role="separator"
              type="button"
              onKeyDown={handleTreeResizeKeyDown}
              onPointerDown={startTreeResize}
            />
          </>
        ) : null}
        <main className="workspaceFilesTablePane" aria-busy={loading}>
          <FileTable
            compact={compact}
            copy={copy}
            entries={paginatedEntries}
            externalDropTargetPath={externalDropTarget?.kind === "directory" ? externalDropTarget.path : null}
            focusedIndex={focusedIndex}
            locale={locale}
            cutRefs={cutRefs}
            selectedRefs={selectedRefs}
            sortAscending={sortAscending}
            sortKey={sortKey}
            ui={ui}
            onAskOperation={askOperation}
            onContextMenu={(entry, point) => {
              setContextMenu({ entry, kind: entry.kind, path: entry.canonicalPath, source: "file-list", ...offsetContextMenuPoint(point) });
            }}
            onFocusedIndexChange={setFocusedIndex}
            onMove={(source, destination) => askOperation("move", source, destination.canonicalPath)}
            onOpenEntry={openEntry}
            onSelectEntry={selectEntry}
            onSelectRefs={(entryRefs) => setSelectedRefs(new Set(entryRefs))}
            onSelectPage={selectPage}
            onSort={changeSort}
            onShowPreview={(entry) => void showPreview(entry)}
          />
          {externalDropTarget?.kind === "current" ? (
            <div className="workspaceFileDropOverlay" role="status">
              <FilesIcon name="upload" size={24} />
              <strong>{ui.dropToCurrentDirectory}</strong>
              <span>{personalInfo.maskText(externalDropTarget.path)}</span>
            </div>
          ) : null}
          {!loading && filesError ? (
            <div className="workspaceEmptyState workspaceFileEmpty" role="alert">
              <p>{copy.filesUnavailable}</p>
              <button type="button" onClick={() => {
                retryRequestRef.current = { hostAlias: selectedHostAlias, path: failedPath };
                setConnectionAttempt((value) => value + 1);
              }}>{copy.retryLoad}</button>
            </div>
          ) : null}
          {!loading && !filesError && page !== null && entries.length === 0 ? <div className="workspaceEmptyState workspaceFileEmpty">{copy.emptyDirectory}</div> : null}
          {!loading && !filesError && page === null ? <div className="workspaceEmptyState workspaceFileEmpty">{copy.loading}</div> : null}
          {loading && page === null ? <div className="workspacePaneState">{copy.loading}</div> : null}
          {page && !filesError ? (
            <footer className="workspaceFilesPagination">
              <span
                aria-label={`${entries.length} ${ui.loadedItems}; ${selectedEntries.length} ${ui.selected}`}
                className="workspaceFilesPaginationSummary"
                title={`${entries.length} ${ui.loadedItems}; ${selectedEntries.length} ${ui.selected}`}
              >
                <strong>{entries.length}</strong><small>{ui.loadedItems}</small><span aria-hidden="true"> · </span><strong>{selectedEntries.length}</strong><small>{ui.selected}</small>
              </span>
              <nav aria-label={ui.pagination}>
                <button aria-label={ui.previousPage} disabled={clientPage === 0} type="button" onClick={() => { setClientPage((value) => Math.max(0, value - 1)); setFocusedIndex(0); }}>‹</button>
                <span>{ui.page} {clientPage + 1} {ui.of} {clientPageCount}</span>
                <button aria-label={ui.nextPage} disabled={clientPage >= clientPageCount - 1} type="button" onClick={() => { setClientPage((value) => Math.min(clientPageCount - 1, value + 1)); setFocusedIndex(0); }}>›</button>
              </nav>
              <label><span className="workspaceVisuallyHidden">{ui.rowsPerPage}</span><select aria-label={ui.rowsPerPage} value={clientPageSize} onChange={(event) => setClientPageSize(Number(event.target.value))}><option value={25}>25</option><option value={50}>50</option><option value={100}>100</option></select></label>
              {page.nextCursor && searchResults === null ? <button className="workspaceLoadMore" disabled={loading} type="button" onClick={() => void loadMore()}>{copy.loadMore}</button> : null}
            </footer>
          ) : null}
          <FileTransferDrawer
            collapsed={transferDrawerCollapsed}
            copy={copy}
            displayNames={transferDisplayNames}
            locale={locale}
            transfers={drawerTransfers}
            ui={ui}
            onCancel={(transfer) => void cancelTransfer(transfer)}
            onPause={(transfer) => void pauseTransfer(transfer)}
            onResume={(transfer) => void resumeTransfer(transfer)}
            onToggle={toggleTransferDrawer}
          />
        </main>
      </div>

      {contextMenu ? createPortal(
        <div className="workspaceContextMenu" role="menu" style={{ left: contextMenu.x, top: contextMenu.y }} onPointerDown={(event) => event.stopPropagation()}>
          {contextMenu.source === "quick-access" ? (
            <button role="menuitem" type="button" onClick={() => removeQuickAccess(contextMenu.path)}>{ui.removeQuickAccess}</button>
          ) : <>
          <button role="menuitem" type="button" onClick={() => {
            if (contextMenu.entry) openEntry(contextMenu.entry);
            else if (fileSession) void navigate(fileSession, contextMenu.path, { manual: true });
            setContextMenu(null);
          }}>{copy.open}</button>
          {contextMenu.kind === "directory" ? <button role="menuitem" type="button" onClick={() => { openFolderInVscode(contextMenu.entry, contextMenu.path); setContextMenu(null); }}>{copy.openFolderInVscode}</button> : null}
          {contextMenu.source === "directory-tree" ? (
            <button
              disabled={quickAccessPaths.includes(contextMenu.path) || fileSession?.homePath === contextMenu.path}
              role="menuitem"
              type="button"
              onClick={() => addQuickAccess(contextMenu.path)}
            >{ui.addQuickAccess}</button>
          ) : null}
          <button role="menuitem" type="button" disabled={!contextMenu.entry} onClick={() => { if (contextMenu.entry) void showPreview(contextMenu.entry); setContextMenu(null); }}>{copy.preview}</button>
          {contextMenu.kind === "file" ? <button role="menuitem" type="button" disabled={!contextMenu.entry} onClick={() => { if (contextMenu.entry) void editEntry(contextMenu.entry); setContextMenu(null); }}>{copy.editFile}</button> : null}
          <button role="menuitem" type="button" disabled={contextTargets.length === 0} onClick={() => { copyEntriesToClipboard(contextTargets); setContextMenu(null); }}>{copy.copyEntry}</button>
          <button role="menuitem" type="button" disabled={contextTargets.length === 0} onClick={() => { cutEntriesToClipboard(contextTargets); setContextMenu(null); }}>{copy.cutEntry}</button>
          <button role="menuitem" type="button" disabled={contextTargets.length === 0} onClick={() => { void download(contextTargets); setContextMenu(null); }}>{copy.download}</button>
          {contextMenu.kind === "directory" ? <button role="menuitem" type="button" onClick={() => { void upload(contextMenu.path); setContextMenu(null); }}>{copy.uploadHere}</button> : null}
          <button role="menuitem" type="button" disabled={!contextMenu.entry?.writable || contextMenu.entry.nameEncoding !== "utf8"} onClick={() => { if (contextMenu.entry) askOperation("rename", contextMenu.entry); setContextMenu(null); }}>{copy.rename}</button>
          <button className="workspaceDangerButton" role="menuitem" type="button" disabled={!contextTargets.some((entry) => entry.writable && entry.nameEncoding === "utf8")} onClick={() => { requestDelete(contextTargets); setContextMenu(null); }}>{copy.delete}</button>
          <button role="menuitem" type="button" onClick={() => { void navigator.clipboard.writeText(contextMenu.path).catch(reportError); setContextMenu(null); }}>{copy.copyPath}</button>
          <button
            disabled={!fileSession || fileSession.targetKind === "local"}
            role="menuitem"
            title={!fileSession || fileSession.targetKind === "local" ? copy.terminalHereUnavailable : copy.openTerminalHere}
            type="button"
            onClick={() => {
              if (fileSession) {
                onOpenTerminalAt(
                  selectedHostAlias,
                  fileSession.fileSessionId,
                  contextMenu.kind === "directory" ? contextMenu.path : parentPath(contextMenu.path)
                );
              }
              setContextMenu(null);
            }}
          >{copy.openTerminalHere}</button>
          </>}
        </div>,
        document.body
      ) : null}

      <FilePreviewDialog copy={copy} preview={preview} onClose={() => setPreview(null)} />
      {editor ? (
        <FileEditorDialog
          copy={copy}
          name={editor.entry.name}
          text={editor.text}
          busy={editor.busy}
          dirty={editor.text !== editor.original}
          onChange={(text) => setEditor((current) => current ? { ...current, text } : current)}
          onCancel={() => setEditor(null)}
          onSave={() => void saveEditor()}
        />
      ) : null}
      <FileDeleteModeDialog
        copy={copy}
        count={pendingDelete?.mode === null ? pendingDelete.entries.length : 0}
        onCancel={() => setPendingDelete(null)}
        onChoose={(mode) => setPendingDelete((current) => current ? { ...current, mode } : current)}
      />
      <FileDeletePreferenceDialog
        copy={copy}
        mode={pendingDelete?.mode ?? null}
        onCancel={() => setPendingDelete(null)}
        onConfirm={confirmDeletePreference}
      />
      <FileOperationDialog
        busy={operationBusy}
        copy={copy}
        nameValid={operationNameValid}
        pending={pendingOperation}
        preview={operationPreview}
        onCancel={() => { setPendingOperation(null); setOperationPreview(null); }}
        onConfirm={() => void confirmOperation()}
        onNameChange={(name) => setPendingOperation((current) => current ? { ...current, name } : current)}
        onPrepare={() => void prepareOperation()}
      />
    </section>
  );
}
