import { act, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import { workspaceCopy } from "./copy";
import {
  FILE_DELETE_MODE_STORAGE_KEY,
  FILE_TRANSFER_COMPLETED_RETENTION_MS,
  FilesPanel,
  formatModifiedAt
} from "./FilesPanel";
import { workspaceHostLabel } from "./hostLabel";
import { resolveFileColumnWidths } from "./files/FileTable";
import { filesUiCopy } from "./files/filesUiCopy";
import { childPath, parentPath } from "./files/fileDisplay";
import { createPersonalInfoMasker } from "../personalInfo";
import { PersonalInfoMaskingProvider } from "../ui/PersonalInfoMasking";
import type {
  RemoteFileEntry,
  WorkspaceApi,
  WorkspaceDirectoryPage,
  WorkspaceFilesSession,
  WorkspaceHost,
  WorkspaceLocalDragStateEvent,
  WorkspaceLocalDropEvent,
  WorkspaceTransfer
} from "./types";

const host: WorkspaceHost = {
  id: "host-1",
  name: "Demo host",
  hostAlias: "demo",
  status: "online"
};

test("Windows drive paths keep their root while navigating", () => {
  expect(parentPath("C:/Users/demo")).toBe("C:/Users");
  expect(parentPath("C:/Users")).toBe("C:/");
  expect(parentPath("C:/")).toBe("C:/");
  expect(childPath("C:/", "Users")).toBe("C:/Users");
});

const session: WorkspaceFilesSession = {
  fileSessionId: "files-1",
  hostAlias: "demo",
  targetKind: "remote",
  homePath: "/home/demo",
  currentPath: "/home/demo",
  state: "connected",
  reason: null
};

const directory: RemoteFileEntry = {
  entryRef: "entry-projects",
  canonicalPath: "/home/demo/projects",
  name: "projects",
  kind: "directory",
  size: "0",
  modifiedAt: "2024-07-03T09:46:40Z",
  permissions: "755",
  uid: "1000",
  gid: "1000",
  symlinkTarget: null,
  fingerprint: "0:1720000000:Directory",
  nameEncoding: "utf8",
  readable: true,
  writable: true
};

const hiddenDirectory: RemoteFileEntry = {
  ...directory,
  entryRef: "entry-hidden",
  canonicalPath: "/home/demo/.config",
  name: ".config"
};

const textFile: RemoteFileEntry = {
  ...directory,
  entryRef: "entry-notes",
  canonicalPath: "/home/demo/notes.txt",
  name: "notes.txt",
  kind: "file",
  size: "128"
};

function page(path: string, entries: RemoteFileEntry[]): WorkspaceDirectoryPage {
  return {
    fileSessionId: session.fileSessionId,
    canonicalPath: path,
    snapshotId: `snapshot-${path}`,
    entries,
    nextCursor: null,
    totalKnown: entries.length,
    truncated: false
  };
}

function renderPanel(entries: RemoteFileEntry[] = [directory], options: { masking?: boolean; compact?: boolean } = {}) {
  const listDirectory = vi.fn(async ({ path }: { path: string | null }) =>
    path === directory.canonicalPath
      ? page(directory.canonicalPath, [])
      : page(session.homePath, entries));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  const panel = (
    <FilesPanel
      activeTerminal={null}
      api={api}
      copy={workspaceCopy.en}
      cwd={null}
      followCwd={false}
      hosts={[host]}
      isActive
      compact={options.compact}
      selectedHostAlias={host.hostAlias}
      onError={vi.fn()}
      onFollowCwdChange={vi.fn()}
      onHostSelected={vi.fn()}
      onOpenTerminalAt={vi.fn()}
      onRecoveryCreated={vi.fn()}
      onViewRecoveries={vi.fn()}
    />
  );
  render(options.masking ? (
    <PersonalInfoMaskingProvider value={createPersonalInfoMasker(true, [{ username: "demo" }])}>
      {panel}
    </PersonalInfoMaskingProvider>
  ) : panel);
  return listDirectory;
}

function errorPanel(
  api: WorkspaceApi,
  onError: ReturnType<typeof vi.fn>,
  options: { isActive?: boolean; selectedHostAlias?: string; hosts?: WorkspaceHost[]; locale?: "en" | "zh" } = {}
) {
  const locale = options.locale ?? "en";
  return (
    <FilesPanel
      activeTerminal={null}
      api={api}
      copy={workspaceCopy[locale]}
      cwd={null}
      followCwd={false}
      hosts={options.hosts ?? [host]}
      isActive={options.isActive ?? true}
      locale={locale}
      selectedHostAlias={options.selectedHostAlias ?? host.hostAlias}
      onError={onError}
      onFollowCwdChange={vi.fn()}
      onHostSelected={vi.fn()}
      onOpenTerminalAt={vi.fn()}
      onRecoveryCreated={vi.fn()}
      onViewRecoveries={vi.fn()}
    />
  );
}

function renderErrorPanel(api: WorkspaceApi, onError: ReturnType<typeof vi.fn>) {
  return render(errorPanel(api, onError));
}

test("invalid remote timestamps never render Invalid Date", () => {
  expect(formatModifiedAt(null)).toBe("—");
  expect(formatModifiedAt("not-a-date")).toBe("—");
  expect(formatModifiedAt("2024-07-03T09:46:40Z")).not.toBe("—");
  expect(formatModifiedAt("2024-07-03T09:46:40Z")).not.toContain("Invalid");
});

test("matching host names and aliases render only once", () => {
  expect(workspaceHostLabel({ ...host, name: "demo" })).toBe("demo");
  expect(workspaceHostLabel(host)).toBe("Demo host · demo");
});

test("quick access stays outside the scrollable directory tree at the card bottom", async () => {
  renderPanel();

  const tree = await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });
  const treeScroll = tree.closest<HTMLElement>(".workspaceFilesTreeScroll");
  const quickAccess = screen.getByRole("heading", { name: filesUiCopy.en.quickAccess }).closest<HTMLElement>(".workspaceFilesQuickAccess");

  expect(treeScroll).not.toContainElement(quickAccess);
  expect(quickAccess?.parentElement).toHaveClass("workspaceFilesTree");
  expect(quickAccess?.previousElementSibling).toBe(treeScroll);
});

test("connected Files host keeps status inside the compact host selector", async () => {
  renderPanel();

  const hostSelect = screen.getByRole("combobox", { name: workspaceCopy.en.host });
  await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });
  const hostPicker = hostSelect.closest<HTMLElement>(".workspaceFilesHostPicker");

  expect(hostPicker).toHaveAttribute("data-connected", "true");
  expect(hostPicker?.querySelector(".workspaceFilesHostStatus")).toHaveAttribute("aria-label", filesUiCopy.en.sftpConnected);
  expect(hostPicker?.textContent).not.toContain(workspaceCopy.en.host);
  expect(screen.queryByText(filesUiCopy.en.sftpConnected)).not.toBeInTheDocument();
});

test("the local Files target opens the real local file session", async () => {
  const stop = () => undefined;
  const localSession: WorkspaceFilesSession = {
    ...session,
    fileSessionId: "local-files",
    hostAlias: "",
    targetKind: "local",
    homePath: "C:/",
    currentPath: "C:/"
  };
  const api = {
    listLocalRoots: vi.fn().mockResolvedValue(["C:/", "E:/", "F:/"]),
    openFiles: vi.fn().mockResolvedValue(localSession),
    listDirectory: vi.fn().mockResolvedValue(page("C:/", [])),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn(), { selectedHostAlias: "" }));

  await waitFor(() => expect(api.openFiles).toHaveBeenCalledWith({ hostAlias: "" }));
  expect(api.listDirectory).toHaveBeenCalledWith(expect.objectContaining({ fileSessionId: "local-files", path: "C:/" }));
});

test("local Files can correct an unavailable drive and navigate to another drive", async () => {
  const stop = () => undefined;
  const localSession: WorkspaceFilesSession = {
    ...session,
    fileSessionId: "local-files",
    hostAlias: "",
    targetKind: "local",
    homePath: "C:/",
    currentPath: "C:/"
  };
  const listDirectory = vi.fn(async ({ path }: { path: string | null }) => {
    if (path === "D:/") throw new Error("local-path-unavailable");
    return page(path === "E:" || path === "E:/" ? "E:/" : "C:/", []);
  });
  const api = {
    listLocalRoots: vi.fn().mockResolvedValue(["C:/", "E:/", "F:/"]),
    openFiles: vi.fn().mockResolvedValue(localSession),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();

  render(errorPanel(api, onError, { selectedHostAlias: "" }));
  const location = await screen.findByRole("textbox", { name: workspaceCopy.en.location });
  await waitFor(() => expect(location).toHaveValue("C:/"));

  fireEvent.change(location, { target: { value: "D:/" } });
  fireEvent.submit(location.closest("form")!);
  await waitFor(() => expect(onError).toHaveBeenCalledWith(expect.objectContaining({ message: "local-path-unavailable" })));
  expect(location).toHaveValue("C:/");
  expect(api.openFiles).toHaveBeenCalledTimes(1);

  const tree = screen.getByRole("tree", { name: filesUiCopy.en.directoryTree });
  expect(within(tree).getByText(filesUiCopy.en.computerRoot)).toBeInTheDocument();
  expect(within(tree).getByTitle("C:/").closest('[role="treeitem"]')).toHaveAttribute("aria-current", "page");
  expect(within(tree).getByTitle("E:/")).toBeInTheDocument();
  expect(within(tree).getByTitle("F:/")).toBeInTheDocument();

  fireEvent.click(within(tree).getByTitle("E:/"));
  await waitFor(() => expect(location).toHaveValue("E:/"));
  expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ fileSessionId: "local-files", path: "E:/" }));
  expect(api.openFiles).toHaveBeenCalledTimes(1);
  expect(within(tree).getByTitle("E:/").closest('[role="treeitem"]')).toHaveAttribute("aria-current", "page");
  expect(within(tree).getByTitle("C:/")).toBeInTheDocument();
});

test("dot-prefixed paths are hidden by default and can be revealed", async () => {
  renderPanel([directory, hiddenDirectory]);

  expect(await screen.findByRole("row", { name: /projects/i })).toBeInTheDocument();
  expect(screen.queryByRole("row", { name: /.config/i })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.showHidden }));
  expect(await screen.findByRole("row", { name: /.config/i })).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.hideHidden }));
  expect(screen.queryByRole("row", { name: /.config/i })).not.toBeInTheDocument();
});

test("the focused two-pane view keeps name, type, size and modified file columns", async () => {
  renderPanel();

  const grid = await screen.findByRole("grid", { name: filesUiCopy.en.tableView });

  expect(within(grid).getByRole("columnheader", { name: workspaceCopy.en.fileName })).toBeInTheDocument();
  expect(within(grid).getByRole("columnheader", { name: workspaceCopy.en.fileType })).toBeInTheDocument();
  expect(within(grid).getByRole("columnheader", { name: workspaceCopy.en.fileSize })).toBeInTheDocument();
  expect(within(grid).getByRole("columnheader", { name: workspaceCopy.en.fileModified })).toBeInTheDocument();
  expect(within(await screen.findByRole("row", { name: /projects/i })).getByText(workspaceCopy.en.directory)).toBeInTheDocument();
  expect(within(grid).queryByRole("columnheader", { name: filesUiCopy.en.permissions })).not.toBeInTheDocument();
  expect(within(grid).queryByRole("columnheader", { name: filesUiCopy.en.owner })).not.toBeInTheDocument();
  expect(screen.queryByRole("complementary", { name: filesUiCopy.en.details })).not.toBeInTheDocument();
});

test("directory tree keeps the real remote user name visible when personal masking is enabled", async () => {
  renderPanel([directory], { masking: true });

  const tree = await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });
  expect(within(tree).getByRole("button", { name: "demo" })).toBeInTheDocument();
  expect(within(tree).queryByRole("button", { name: "d*" })).not.toBeInTheDocument();
});

test("Files more-actions menu closes from an outside press or Escape", async () => {
  renderPanel();
  await screen.findByRole("grid", { name: filesUiCopy.en.tableView });

  const menu = document.querySelector<HTMLDetailsElement>(".workspaceFilesMoreActions");
  const toggle = menu?.querySelector<HTMLElement>("summary");
  expect(menu).not.toBeNull();
  expect(toggle).not.toBeNull();
  if (!menu || !toggle) throw new Error("Files more-actions menu is unavailable");

  fireEvent.click(toggle);
  expect(menu.open).toBe(true);
  fireEvent.pointerDown(toggle);
  expect(menu.open).toBe(true);

  fireEvent.pointerDown(document.body);
  expect(menu.open).toBe(false);

  fireEvent.click(toggle);
  expect(menu.open).toBe(true);
  fireEvent.keyDown(document, { key: "Escape" });
  expect(menu.open).toBe(false);
  expect(toggle).toHaveFocus();
});

test("the More menu opens the current directory in VS Code", async () => {
  const stop = () => undefined;
  const openFolderInVscode = vi.fn().mockResolvedValue(undefined);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    openFolderInVscode,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  await screen.findByRole("row", { name: /projects/i });
  const moreActions = document.querySelector<HTMLDetailsElement>(".workspaceFilesMoreActions");
  expect(moreActions).not.toBeNull();
  if (!moreActions) throw new Error("Files more-actions menu is unavailable");
  fireEvent.click(moreActions.querySelector("summary")!);
  fireEvent.click(within(moreActions).getByRole("button", { name: workspaceCopy.en.openFolderInVscode }));

  await waitFor(() => expect(openFolderInVscode).toHaveBeenCalledWith({
    fileSessionId: session.fileSessionId,
    path: session.homePath,
    entryRef: null
  }));
});

test("a directory context menu opens that directory in VS Code", async () => {
  const stop = () => undefined;
  const openFolderInVscode = vi.fn().mockResolvedValue(undefined);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    openFolderInVscode,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  fireEvent.contextMenu(await screen.findByRole("row", { name: /projects/i }), { clientX: 10, clientY: 40 });
  const contextMenu = screen.getByRole("menu");
  expect(contextMenu).toHaveStyle({ left: "10px", top: "40px" });
  expect(contextMenu.parentElement).toBe(document.body);
  fireEvent.click(within(contextMenu).getByRole("menuitem", { name: workspaceCopy.en.openFolderInVscode }));

  await waitFor(() => expect(openFolderInVscode).toHaveBeenCalledWith({
    fileSessionId: session.fileSessionId,
    path: directory.canonicalPath,
    entryRef: directory.entryRef
  }));
});

test("a directory tree context menu opens that folder in VS Code", async () => {
  const stop = () => undefined;
  const openFolderInVscode = vi.fn().mockResolvedValue(undefined);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    openFolderInVscode,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  const tree = await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });
  fireEvent.contextMenu(within(tree).getByTitle(directory.canonicalPath), { clientX: 18, clientY: 24 });
  fireEvent.click(screen.getByRole("menuitem", { name: workspaceCopy.en.openFolderInVscode }));

  await waitFor(() => expect(openFolderInVscode).toHaveBeenCalledWith({
    fileSessionId: session.fileSessionId,
    path: directory.canonicalPath,
    entryRef: directory.entryRef
  }));
});

test("directory tree keeps the file-list actions and adds Quick access", async () => {
  renderPanel();

  fireEvent.contextMenu(await screen.findByRole("row", { name: /projects/i }), { clientX: 10, clientY: 10 });
  const fileMenu = screen.getByRole("menu");
  const fileActions = within(fileMenu).getAllByRole("menuitem").map((item) => item.textContent);
  expect(within(fileMenu).getAllByRole("menuitem", { name: workspaceCopy.en.copyEntry })).toHaveLength(1);

  fireEvent.pointerDown(document.body);
  const tree = screen.getByRole("tree", { name: filesUiCopy.en.directoryTree });
  fireEvent.contextMenu(within(tree).getByTitle(directory.canonicalPath), { clientX: 18, clientY: 24 });
  const treeMenu = screen.getByRole("menu");
  const treeActions = within(treeMenu).getAllByRole("menuitem").map((item) => item.textContent);

  expect(treeActions.filter((action) => action !== filesUiCopy.en.addQuickAccess)).toEqual(fileActions);
  expect(within(treeMenu).getByRole("menuitem", { name: filesUiCopy.en.addQuickAccess })).toBeEnabled();
  expect(within(treeMenu).getAllByRole("menuitem", { name: workspaceCopy.en.copyEntry })).toHaveLength(1);
});

test("a pinned directory has a one-action Quick access context menu and can be removed", async () => {
  renderPanel();
  const tree = await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });
  const treeDirectory = within(tree).getByTitle(directory.canonicalPath);

  fireEvent.contextMenu(treeDirectory, { clientX: 18, clientY: 24 });
  fireEvent.click(screen.getByRole("menuitem", { name: filesUiCopy.en.addQuickAccess }));

  const quickAccess = screen.getByRole("heading", { name: filesUiCopy.en.quickAccess }).closest<HTMLElement>(".workspaceFilesQuickAccess");
  expect(quickAccess).not.toBeNull();
  if (!quickAccess) throw new Error("Quick access is unavailable");
  const pinnedDirectory = within(quickAccess).getByTitle(directory.canonicalPath);
  expect(pinnedDirectory).toHaveTextContent(directory.name);

  fireEvent.contextMenu(pinnedDirectory, { clientX: 30, clientY: 50 });
  const quickAccessMenu = screen.getByRole("menu");
  const items = within(quickAccessMenu).getAllByRole("menuitem");
  expect(items).toHaveLength(1);
  expect(items[0]).toHaveAccessibleName(filesUiCopy.en.removeQuickAccess);

  fireEvent.click(items[0]);
  expect(within(quickAccess).queryByTitle(directory.canonicalPath)).not.toBeInTheDocument();
});

test("Ctrl+C copies selected files and Ctrl+V pastes them", async () => {
  const stop = () => undefined;
  const copyEntries = vi.fn().mockResolvedValue([]);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    copyEntries,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  fireEvent.click(await screen.findByRole("row", { name: /projects/i }));
  fireEvent.keyDown(window, { key: "c", ctrlKey: true });
  fireEvent.keyDown(window, { key: "v", ctrlKey: true });

  await waitFor(() => expect(copyEntries).toHaveBeenCalledWith({
    sourceFileSessionId: session.fileSessionId,
    destinationFileSessionId: session.fileSessionId,
    sourceEntryRefs: [directory.entryRef],
    destinationPath: session.homePath
  }));
});

test("dragging from a file name cell selects every intersecting file row", async () => {
  renderPanel([directory, textFile]);
  const grid = await screen.findByRole("grid", { name: filesUiCopy.en.tableView });
  const rows = within(grid).getAllByRole("row").slice(1);
  expect(rows).toHaveLength(2);

  vi.spyOn(rows[0], "getBoundingClientRect").mockReturnValue({
    x: 0, y: 40, left: 0, top: 40, right: 500, bottom: 80, width: 500, height: 40,
    toJSON: () => ({})
  });
  vi.spyOn(rows[1], "getBoundingClientRect").mockReturnValue({
    x: 0, y: 80, left: 0, top: 80, right: 500, bottom: 120, width: 500, height: 40,
    toJSON: () => ({})
  });

  const nameCell = within(rows[1]).getAllByRole("gridcell")[1];
  fireEvent.pointerDown(nameCell, { button: 0, clientX: 490, clientY: 110 });
  fireEvent.pointerMove(window, { clientX: 10, clientY: 45 });

  expect(rows[0]).toHaveAttribute("aria-selected", "true");
  expect(rows[1]).toHaveAttribute("aria-selected", "true");
  const selectionRectangle = document.querySelector<HTMLElement>(".workspaceFileSelectionRectangle");
  expect(selectionRectangle).not.toBeNull();
  expect(selectionRectangle?.parentElement).toBe(document.body);
  expect(selectionRectangle).toHaveStyle({ left: "10px", top: "45px", width: "480px", height: "65px" });

  fireEvent.pointerUp(window);
  expect(document.querySelector(".workspaceFileSelectionRectangle")).toBeNull();

  fireEvent.click(nameCell);
  expect(rows[0]).toHaveAttribute("aria-selected", "true");
  expect(rows[1]).toHaveAttribute("aria-selected", "true");
});

test("only the file icon is the internal drag handle", async () => {
  renderPanel([directory]);
  const row = await screen.findByRole("row", { name: /projects/i });
  const nameCell = within(row).getAllByRole("gridcell")[1];
  const dragHandle = nameCell.querySelector<HTMLElement>(".workspaceFileDragHandle");
  expect(nameCell).not.toHaveAttribute("draggable");
  expect(dragHandle).toHaveAttribute("draggable", "true");
  expect(dragHandle).toHaveAttribute("title", filesUiCopy.en.dragToMove);
  expect(row).not.toHaveAttribute("draggable");
});

test("Ctrl+X cuts all selected files and Ctrl+V moves them together", async () => {
  const stop = () => undefined;
  const moveEntries = vi.fn().mockResolvedValue([]);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory, textFile])),
    moveEntries,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  fireEvent.click(await screen.findByRole("row", { name: /projects/i }));
  fireEvent.click(screen.getByRole("row", { name: /notes\.txt/i }), { ctrlKey: true });
  fireEvent.keyDown(window, { key: "x", ctrlKey: true });
  fireEvent.keyDown(window, { key: "v", ctrlKey: true });

  await waitFor(() => expect(moveEntries).toHaveBeenCalledWith({
    sourceFileSessionId: session.fileSessionId,
    destinationFileSessionId: session.fileSessionId,
    sourceEntryRefs: [directory.entryRef, textFile.entryRef],
    destinationPath: session.homePath
  }));
});

test("right-clicking one selected row keeps the group for context-menu copy", async () => {
  const stop = () => undefined;
  const copyEntries = vi.fn().mockResolvedValue([]);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory, textFile])),
    copyEntries,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  const first = await screen.findByRole("row", { name: /projects/i });
  const second = screen.getByRole("row", { name: /notes\.txt/i });
  fireEvent.click(first);
  fireEvent.click(second, { ctrlKey: true });
  fireEvent.contextMenu(second, { clientX: 10, clientY: 10 });
  fireEvent.click(screen.getByRole("menuitem", { name: workspaceCopy.en.copyEntry }));
  fireEvent.keyDown(window, { key: "v", ctrlKey: true });

  await waitFor(() => expect(copyEntries).toHaveBeenCalledWith(expect.objectContaining({
    sourceEntryRefs: [directory.entryRef, textFile.entryRef]
  })));
});

test("the More menu delete action offers backup mode and asks whether to remember it", async () => {
  window.localStorage.removeItem(FILE_DELETE_MODE_STORAGE_KEY);
  const stop = () => undefined;
  const recovery = {
    recoveryId: "recovery-delete-1",
    hostAlias: host.hostAlias,
    operation: "delete" as const,
    originalPath: textFile.canonicalPath,
    recoveryPath: "/home/demo/.codexhub-workspace-backups/recovery-delete-1/payload",
    createdAt: "2026-08-03T08:00:00.000Z",
    state: "available" as const,
    taskId: "task-delete-1",
    reason: null
  };
  const listDirectory = vi.fn()
    .mockResolvedValueOnce(page(session.homePath, [textFile]))
    .mockResolvedValue(page(session.homePath, []));
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory,
    prepareFileOperation: vi.fn().mockResolvedValue({
      token: "delete-token",
      operation: "delete",
      hostAlias: host.hostAlias,
      sourcePath: textFile.canonicalPath,
      targetPath: null,
      backupPath: recovery.recoveryPath,
      impactSummary: "delete",
      expiresAt: "2026-08-03T08:01:00.000Z",
      requiresBackup: false
    }),
    confirmFileOperation: vi.fn().mockResolvedValue({ taskId: recovery.taskId, recovery, destinationEntry: null }),
    prepareRecoveryPurge: vi.fn(),
    purgeRecovery: vi.fn(),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  try {
    render(errorPanel(api, vi.fn()));
    fireEvent.click(await screen.findByRole("row", { name: /notes\.txt/i }));
    const menu = document.querySelector<HTMLDetailsElement>(".workspaceFilesMoreActions");
    expect(menu).not.toBeNull();
    if (!menu) throw new Error("Files more-actions menu is unavailable");
    fireEvent.click(menu.querySelector("summary")!);
    const menuGroup = within(menu).getByRole("group", { name: filesUiCopy.en.moreActions });
    const menuButtons = within(menuGroup).getAllByRole("button");
    const newFolderIndex = menuButtons.findIndex((button) => button.getAttribute("aria-label") === workspaceCopy.en.newFolder);
    const deleteIndex = menuButtons.findIndex((button) => button.getAttribute("aria-label") === workspaceCopy.en.delete);
    expect(deleteIndex).toBe(newFolderIndex + 1);

    fireEvent.click(within(menuGroup).getByRole("button", { name: workspaceCopy.en.delete }));
    expect(screen.getByRole("alertdialog", { name: workspaceCopy.en.deleteModeTitle })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.backupDelete }));
    expect(screen.getByRole("alertdialog", { name: workspaceCopy.en.deletePreferenceTitle })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.askDeleteNextTime }));

    await waitFor(() => expect(api.confirmFileOperation).toHaveBeenCalledWith({ token: "delete-token" }));
    expect(api.prepareRecoveryPurge).not.toHaveBeenCalled();
    expect(window.localStorage.getItem(FILE_DELETE_MODE_STORAGE_KEY)).toBeNull();
    await waitFor(() => expect(listDirectory).toHaveBeenCalledTimes(2));
  } finally {
    window.localStorage.removeItem(FILE_DELETE_MODE_STORAGE_KEY);
  }
});

test("direct delete purges the temporary recovery and persists the chosen default", async () => {
  window.localStorage.removeItem(FILE_DELETE_MODE_STORAGE_KEY);
  const stop = () => undefined;
  const recovery = {
    recoveryId: "recovery-direct-1",
    hostAlias: host.hostAlias,
    operation: "delete" as const,
    originalPath: textFile.canonicalPath,
    recoveryPath: "/home/demo/.codexhub-workspace-backups/recovery-direct-1/payload",
    createdAt: "2026-08-03T08:00:00.000Z",
    state: "available" as const,
    taskId: "task-direct-1",
    reason: null
  };
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [textFile])),
    prepareFileOperation: vi.fn().mockResolvedValue({ token: "direct-token" }),
    confirmFileOperation: vi.fn().mockResolvedValue({ taskId: recovery.taskId, recovery, destinationEntry: null }),
    prepareRecoveryPurge: vi.fn().mockResolvedValue({
      token: "purge-token",
      recoveryId: recovery.recoveryId,
      hostAlias: host.hostAlias,
      recoveryPath: recovery.recoveryPath,
      expiresAt: "2026-08-03T08:01:00.000Z"
    }),
    purgeRecovery: vi.fn().mockResolvedValue({ ...recovery, state: "purged" }),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;

  try {
    render(errorPanel(api, vi.fn()));
    fireEvent.contextMenu(await screen.findByRole("row", { name: /notes\.txt/i }), { clientX: 10, clientY: 10 });
    fireEvent.click(screen.getByRole("menuitem", { name: workspaceCopy.en.delete }));
    fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.directDelete }));
    fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.setDeleteDefault }));

    await waitFor(() => expect(api.prepareRecoveryPurge).toHaveBeenCalledWith({ recoveryId: recovery.recoveryId }));
    expect(api.purgeRecovery).toHaveBeenCalledWith({ token: "purge-token" });
    expect(window.localStorage.getItem(FILE_DELETE_MODE_STORAGE_KEY)).toBe("direct");

    fireEvent.contextMenu(await screen.findByRole("row", { name: /notes\.txt/i }), { clientX: 10, clientY: 10 });
    fireEvent.click(screen.getByRole("menuitem", { name: workspaceCopy.en.delete }));
    await waitFor(() => expect(api.confirmFileOperation).toHaveBeenCalledTimes(2));
    expect(screen.queryByRole("alertdialog", { name: workspaceCopy.en.deleteModeTitle })).not.toBeInTheDocument();
  } finally {
    window.localStorage.removeItem(FILE_DELETE_MODE_STORAGE_KEY);
  }
});

test("directory tree and file columns expose pointer resize controls", async () => {
  renderPanel();

  await screen.findByRole("grid", { name: filesUiCopy.en.tableView });
  const explorer = document.querySelector<HTMLDivElement>(".workspaceFilesExplorer");
  expect(explorer).not.toBeNull();
  if (!explorer) throw new Error("Files explorer is unavailable");
  Object.defineProperty(explorer, "clientWidth", { configurable: true, value: 1000 });

  const treeResizeHandle = screen.getByRole("separator", { name: filesUiCopy.en.resizeDirectoryTree });
  fireEvent.pointerDown(treeResizeHandle, { button: 0, clientX: 248 });
  fireEvent.pointerMove(window, { clientX: 320 });
  expect(explorer.style.getPropertyValue("--workspace-files-tree-width")).toBe("320px");
  fireEvent.pointerUp(window);

  const nameResizeHandle = screen.getByRole("separator", { name: `${filesUiCopy.en.resizeColumn}: ${workspaceCopy.en.fileName}` });
  fireEvent.pointerDown(nameResizeHandle, { button: 0, clientX: 100 });
  fireEvent.pointerMove(window, { clientX: 180 });
  expect(screen.getByRole("grid", { name: filesUiCopy.en.tableView }).style.getPropertyValue("--workspace-file-name-column-width")).toBe("420px");
  fireEvent.pointerUp(window);
});

test("file column resize follows the pointer after responsive fitting", async () => {
  const clientWidthDescriptor = Object.getOwnPropertyDescriptor(HTMLElement.prototype, "clientWidth");
  Object.defineProperty(HTMLElement.prototype, "clientWidth", {
    configurable: true,
    get() { return this.classList.contains("workspaceFileGrid") ? 900 : 0; }
  });

  try {
    renderPanel();
    const grid = await screen.findByRole("grid", { name: filesUiCopy.en.tableView });
    await waitFor(() => expect(grid.style.getPropertyValue("--workspace-file-name-column-width")).toBe("436px"));
    const handle = screen.getByRole("separator", { name: `${filesUiCopy.en.resizeColumn}: ${workspaceCopy.en.fileName}` });

    fireEvent.pointerDown(handle, { button: 0, clientX: 100 });
    fireEvent.pointerMove(window, { clientX: 180 });
    expect(grid.style.getPropertyValue("--workspace-file-name-column-width")).toBe("516px");
    expect(grid.style.getPropertyValue("--workspace-file-table-width")).toBe("980px");
    fireEvent.pointerUp(window);

    fireEvent.pointerDown(handle, { button: 0, clientX: 180 });
    fireEvent.pointerMove(window, { clientX: 100 });
    expect(grid.style.getPropertyValue("--workspace-file-name-column-width")).toBe("436px");
    expect(grid.style.getPropertyValue("--workspace-file-table-width")).toBe("900px");
    fireEvent.pointerUp(window);
  } finally {
    if (clientWidthDescriptor) {
      Object.defineProperty(HTMLElement.prototype, "clientWidth", clientWidthDescriptor);
    } else {
      Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
    }
  }
});

test("compact Files starts with its directory tree collapsed and can open it on demand", async () => {
  renderPanel([directory], { compact: true });

  await screen.findByRole("grid", { name: filesUiCopy.en.tableView });
  expect(screen.queryByRole("tree", { name: filesUiCopy.en.directoryTree })).not.toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.showTree }));
  expect(await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree })).toBeInTheDocument();
});

test("entering compact Files closes the tree once without overriding a later user choice", async () => {
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const panelProps = {
    activeTerminal: null,
    api,
    copy: workspaceCopy.en,
    cwd: null,
    followCwd: false,
    hosts: [host],
    isActive: true,
    selectedHostAlias: host.hostAlias,
    onError: vi.fn(),
    onFollowCwdChange: vi.fn(),
    onHostSelected: vi.fn(),
    onOpenTerminalAt: vi.fn(),
    onRecoveryCreated: vi.fn(),
    onViewRecoveries: vi.fn()
  };
  const renderPanelForMode = (compact: boolean) => <FilesPanel {...panelProps} compact={compact} />;
  const view = render(renderPanelForMode(false));

  expect(await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree })).toBeInTheDocument();
  view.rerender(renderPanelForMode(true));
  await waitFor(() => expect(screen.queryByRole("tree", { name: filesUiCopy.en.directoryTree })).not.toBeInTheDocument());

  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.showTree }));
  expect(await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree })).toBeInTheDocument();

  view.rerender(renderPanelForMode(true));
  expect(await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree })).toBeInTheDocument();
});

test("file columns fit the compact table viewport without horizontal overflow", () => {
  const widths = resolveFileColumnWidths({ name: 340, type: 104, size: 124, modified: 188 }, 230, true);

  expect(widths.name).toBeGreaterThan(0);
  expect(widths.type).toBeGreaterThan(0);
  expect(widths.size).toBeGreaterThan(0);
  expect(widths.modified).toBeGreaterThan(0);
  expect(widths.name + widths.type + widths.size + widths.modified).toBe(182);
});

test("directory tree expansion lazily lists the selected real canonical path", async () => {
  const listDirectory = renderPanel();
  const tree = await screen.findByRole("tree", { name: filesUiCopy.en.directoryTree });

  fireEvent.click(within(tree).getByRole("button", { name: `${filesUiCopy.en.expandDirectory}: ${directory.name}` }));

  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({
    fileSessionId: session.fileSessionId,
    path: directory.canonicalPath,
    sort: "name",
    direction: "asc"
  })));
});

test("client pagination and page checkboxes keep selection explicit", async () => {
  const entries = Array.from({ length: 30 }, (_, index): RemoteFileEntry => ({
    ...directory,
    entryRef: `entry-${index}`,
    canonicalPath: `${session.homePath}/item-${index}`,
    name: `item-${index}`,
    kind: "file"
  }));
  renderPanel(entries);
  await screen.findByRole("row", { name: /item-0/i });

  fireEvent.change(screen.getByRole("combobox", { name: filesUiCopy.en.rowsPerPage }), { target: { value: "25" } });
  await waitFor(() => expect(screen.queryByRole("row", { name: /item-29/i })).not.toBeInTheDocument());
  fireEvent.click(screen.getByRole("checkbox", { name: filesUiCopy.en.selectAllPage }));
  expect(screen.getByLabelText(/25 selected/)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.nextPage }));
  expect(await screen.findByRole("row", { name: /item-29/i })).toBeInTheDocument();
});

test("file column headers sort ascending first and reverse on the second click", async () => {
  const listDirectory = renderPanel();
  const nameHeader = await screen.findByRole("columnheader", { name: workspaceCopy.en.fileName });
  const typeHeader = screen.getByRole("columnheader", { name: workspaceCopy.en.fileType });
  const sizeHeader = screen.getByRole("columnheader", { name: workspaceCopy.en.fileSize });
  const modifiedHeader = screen.getByRole("columnheader", { name: workspaceCopy.en.fileModified });

  expect(typeHeader).toHaveAttribute("aria-sort", "ascending");
  expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "type", direction: "asc" }));

  fireEvent.click(within(typeHeader).getByRole("button", { name: workspaceCopy.en.fileType }));
  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "type", direction: "desc" })));
  expect(typeHeader).toHaveAttribute("aria-sort", "descending");

  fireEvent.click(within(nameHeader).getByRole("button", { name: workspaceCopy.en.fileName }));
  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "name", direction: "asc" })));
  expect(nameHeader).toHaveAttribute("aria-sort", "ascending");

  fireEvent.click(within(sizeHeader).getByRole("button", { name: workspaceCopy.en.fileSize }));
  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "size", direction: "asc" })));
  expect(sizeHeader).toHaveAttribute("aria-sort", "ascending");

  fireEvent.click(within(sizeHeader).getByRole("button", { name: workspaceCopy.en.fileSize }));
  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "size", direction: "desc" })));
  expect(sizeHeader).toHaveAttribute("aria-sort", "descending");

  fireEvent.click(within(modifiedHeader).getByRole("button", { name: workspaceCopy.en.fileModified }));
  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ sort: "modified", direction: "asc" })));
  expect(modifiedHeader).toHaveAttribute("aria-sort", "ascending");
});

test("Files-specific redesign copy follows the requested locale", async () => {
  const stop = () => undefined;
  const api = { openFiles: vi.fn(), events: { onFileSearchUpdated: vi.fn().mockReturnValue(stop), onLocalDrop: vi.fn().mockReturnValue(stop) } } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn(), { selectedHostAlias: "", locale: "zh" }));

  expect(await screen.findByRole("complementary", { name: filesUiCopy.zh.directoryTree })).toBeInTheDocument();
  expect(screen.queryByRole("complementary", { name: filesUiCopy.zh.details })).not.toBeInTheDocument();
});

test.each(["double-click", "enter"])("%s opens a directory by canonical POSIX path", async (action) => {
  const listDirectory = renderPanel();
  const row = await screen.findByRole("row", { name: /projects/i });

  if (action === "double-click") fireEvent.doubleClick(row);
  else fireEvent.keyDown(row, { key: "Enter" });

  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({
    fileSessionId: session.fileSessionId,
    path: directory.canonicalPath
  })));
});

test("initial directory failure is visible and successful retry clears it", async () => {
  const listDirectory = vi.fn()
    .mockRejectedValueOnce(new Error("invalid-remote-path"))
    .mockResolvedValueOnce(page(session.homePath, []));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    closeFiles: vi.fn().mockResolvedValue(undefined),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  renderErrorPanel(api, onError);

  await waitFor(() => expect(screen.getByRole("alert")).toHaveTextContent(workspaceCopy.en.filesUnavailable));
  expect(screen.queryByText(workspaceCopy.en.emptyDirectory)).not.toBeInTheDocument();
  expect(onError).toHaveBeenCalledWith(expect.objectContaining({ message: "invalid-remote-path" }));

  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.retryLoad }));

  await waitFor(() => expect(listDirectory).toHaveBeenCalledTimes(2));
  await waitFor(() => expect(screen.queryByRole("alert")).not.toBeInTheDocument());
  expect(api.closeFiles).toHaveBeenCalledWith({ fileSessionId: session.fileSessionId });
  expect(screen.getByText(workspaceCopy.en.emptyDirectory)).toBeInTheDocument();
});

test("mode changes preserve the active host directory without reopening Files", async () => {
  const listDirectory = vi.fn(async ({ path }: { path: string | null }) =>
    path === directory.canonicalPath
      ? page(directory.canonicalPath, [])
      : page(session.homePath, [directory]));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  const view = renderErrorPanel(api, onError);

  fireEvent.doubleClick(await screen.findByRole("row", { name: /projects/i }));
  await screen.findByDisplayValue(directory.canonicalPath);
  const callsBeforeModeChange = listDirectory.mock.calls.length;

  view.rerender(errorPanel(api, onError, { isActive: false }));
  view.rerender(errorPanel(api, onError, { isActive: true }));

  await screen.findByDisplayValue(directory.canonicalPath);
  expect(api.openFiles).toHaveBeenCalledTimes(1);
  expect(listDirectory).toHaveBeenCalledTimes(callsBeforeModeChange);
});

test("parent callback rerenders do not reopen Files at the home path", async () => {
  const listDirectory = vi.fn(async ({ path }: { path: string | null }) =>
    path === directory.canonicalPath
      ? page(directory.canonicalPath, [])
      : page(session.homePath, [directory]));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const stableProps = {
    activeTerminal: null,
    api,
    copy: workspaceCopy.en,
    cwd: null,
    followCwd: false,
    hosts: [host],
    isActive: true,
    selectedHostAlias: host.hostAlias,
    onFollowCwdChange: vi.fn(),
    onHostSelected: vi.fn(),
    onOpenTerminalAt: vi.fn(),
    onRecoveryCreated: vi.fn(),
    onViewRecoveries: vi.fn()
  };
  const view = render(<FilesPanel {...stableProps} onError={vi.fn()} />);

  fireEvent.doubleClick(await screen.findByRole("row", { name: /projects/i }));
  await screen.findByDisplayValue(directory.canonicalPath);
  const callsBeforeParentRerender = listDirectory.mock.calls.length;

  // App recreates its error reporter when transfer progress updates render.
  view.rerender(<FilesPanel {...stableProps} onError={vi.fn()} />);

  await screen.findByDisplayValue(directory.canonicalPath);
  expect(api.openFiles).toHaveBeenCalledTimes(1);
  expect(listDirectory).toHaveBeenCalledTimes(callsBeforeParentRerender);
});

test("an unfinished Files view reloads after returning to the mode", async () => {
  let resolveInitialDirectory: (value: WorkspaceDirectoryPage) => void = () => undefined;
  const listDirectory = vi.fn()
    .mockImplementationOnce(() => new Promise<WorkspaceDirectoryPage>((resolve) => { resolveInitialDirectory = resolve; }))
    .mockResolvedValueOnce(page(session.homePath, [directory]));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    closeFiles: vi.fn().mockResolvedValue(undefined),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  const view = renderErrorPanel(api, onError);

  await waitFor(() => expect(listDirectory).toHaveBeenCalledTimes(1));
  view.rerender(errorPanel(api, onError, { isActive: false }));
  resolveInitialDirectory(page(session.homePath, [directory]));
  view.rerender(errorPanel(api, onError, { isActive: true }));

  await waitFor(() => expect(api.openFiles).toHaveBeenCalledTimes(2));
  expect(await screen.findByRole("row", { name: /projects/i })).toBeInTheDocument();
});

test("changing hosts while Files is inactive does not restore the previous host view", async () => {
  const otherHost: WorkspaceHost = { ...host, id: "host-2", name: "Other host", hostAlias: "other" };
  const otherSession = { ...session, fileSessionId: "files-2", hostAlias: otherHost.hostAlias };
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn(async ({ hostAlias }: { hostAlias: string }) => hostAlias === otherHost.hostAlias ? otherSession : session),
    listDirectory: vi.fn(async ({ fileSessionId }: { fileSessionId: string }) =>
      fileSessionId === otherSession.fileSessionId
        ? { ...page("/home/other", []), fileSessionId: otherSession.fileSessionId }
        : page(session.homePath, [directory])),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  const hosts = [host, otherHost];
  const view = render(errorPanel(api, onError, { hosts }));

  await screen.findByRole("row", { name: /projects/i });
  view.rerender(errorPanel(api, onError, { hosts, isActive: false }));
  view.rerender(errorPanel(api, onError, { hosts, isActive: false, selectedHostAlias: otherHost.hostAlias }));
  view.rerender(errorPanel(api, onError, { hosts, selectedHostAlias: otherHost.hostAlias }));

  await screen.findByText(workspaceCopy.en.emptyDirectory);
  expect(api.openFiles).toHaveBeenLastCalledWith({ hostAlias: otherHost.hostAlias });
  expect(screen.queryByRole("row", { name: /projects/i })).not.toBeInTheDocument();
});

test("a failed host view is reopened and keeps a visible retry action across host switches", async () => {
  const otherHost: WorkspaceHost = { ...host, id: "host-2", name: "Other host", hostAlias: "other" };
  const otherSession = { ...session, fileSessionId: "files-2", hostAlias: otherHost.hostAlias };
  const listDirectory = vi.fn(async ({ fileSessionId }: { fileSessionId: string }) => {
    if (fileSessionId === session.fileSessionId) throw new Error("directory-open-failed");
    return { ...page("/home/other", []), fileSessionId: otherSession.fileSessionId };
  });
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn(async ({ hostAlias }: { hostAlias: string }) => hostAlias === otherHost.hostAlias ? otherSession : session),
    closeFiles: vi.fn().mockResolvedValue(undefined),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  const hosts = [host, otherHost];
  const view = render(errorPanel(api, onError, { hosts }));

  await screen.findByRole("alert");
  view.rerender(errorPanel(api, onError, { hosts, selectedHostAlias: otherHost.hostAlias }));
  await screen.findByText(workspaceCopy.en.emptyDirectory);
  view.rerender(errorPanel(api, onError, { hosts, selectedHostAlias: host.hostAlias }));

  expect(await screen.findByRole("alert")).toHaveTextContent(workspaceCopy.en.filesUnavailable);
  expect(api.closeFiles).toHaveBeenCalledWith({ fileSessionId: session.fileSessionId });
  expect(api.openFiles).toHaveBeenCalledTimes(3);
  expect(screen.getByRole("button", { name: workspaceCopy.en.retryLoad })).toBeInTheDocument();
});

test("retry after a child-directory failure requests the same directory", async () => {
  let childAttempts = 0;
  const listDirectory = vi.fn(async ({ path }: { path: string | null }) => {
    if (path === directory.canonicalPath && childAttempts++ === 0) {
      throw new Error("directory-open-failed");
    }
    return path === directory.canonicalPath
      ? page(directory.canonicalPath, [])
      : page(session.homePath, [directory]);
  });
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    closeFiles: vi.fn().mockResolvedValue(undefined),
    listDirectory,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const onError = vi.fn();
  renderErrorPanel(api, onError);

  fireEvent.doubleClick(await screen.findByRole("row", { name: /projects/i }));
  await screen.findByRole("alert");
  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.retryLoad }));

  await waitFor(() => expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({
    path: directory.canonicalPath
  })));
  expect(api.closeFiles).toHaveBeenCalledWith({ fileSessionId: session.fileSessionId });
  await screen.findByDisplayValue(directory.canonicalPath);
  expect(screen.getByText(workspaceCopy.en.emptyDirectory)).toBeInTheDocument();
});

function sampleTransfer(
  state: WorkspaceTransfer["state"],
  overrides: Partial<WorkspaceTransfer> = {}
): WorkspaceTransfer {
  return {
    transferId: "transfer-upload-1",
    revision: state === "completed" ? 2 : 1,
    createdAt: "2026-08-03T08:00:00.000Z",
    updatedAt: state === "completed" ? "2026-08-03T08:00:02.000Z" : "2026-08-03T08:00:00.000Z",
    direction: "upload",
    hostAlias: host.hostAlias,
    sourceLabel: "Local selection",
    targetLabel: session.homePath,
    state,
    bytes: state === "completed" ? "100" : "20",
    total: "100",
    speedBytesPerSecond: state === "completed" ? null : "25",
    etaSeconds: state === "completed" ? null : 3,
    attempt: 1,
    resumable: true,
    resumeOffset: null,
    fingerprintState: "unchecked",
    errorCode: null,
    errorMessage: null,
    taskId: null,
    conflictRevision: null,
    capabilities: {
      canPause: state === "running",
      canResume: state === "paused",
      canCancel: state !== "completed",
      canRetry: false,
      canRestart: false
    },
    ...overrides
  };
}

test("native file drag highlights a directory and uploads to that exact target", async () => {
  let dragHandler: ((event: WorkspaceLocalDragStateEvent) => void) | null = null;
  let dropHandler: ((event: WorkspaceLocalDropEvent) => void) | null = null;
  const enqueueTransfers = vi.fn().mockResolvedValue([sampleTransfer("running")]);
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    enqueueTransfers,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDragState: vi.fn((handler) => { dragHandler = handler; return stop; }),
      onLocalDrop: vi.fn((handler) => { dropHandler = handler; return stop; })
    }
  } as unknown as WorkspaceApi;

  render(errorPanel(api, vi.fn()));
  const row = await screen.findByRole("row", { name: /projects/i });
  await waitFor(() => expect(dragHandler).not.toBeNull());
  const originalElementFromPoint = document.elementFromPoint;
  Object.defineProperty(document, "elementFromPoint", { configurable: true, value: vi.fn().mockReturnValue(row) });
  try {
    act(() => dragHandler?.({ phase: "over", clientX: 40, clientY: 80 }));
    expect(row).toHaveAttribute("data-external-drop-target", "true");
    act(() => dragHandler?.({ phase: "drop", clientX: 40, clientY: 80 }));
    act(() => dropHandler?.({ grants: [{ grantId: "grant-1", displayName: "notes.txt", kind: "file", expiresAt: "2026-08-03T08:05:00.000Z" }] }));
    await waitFor(() => expect(enqueueTransfers).toHaveBeenCalledWith(expect.objectContaining({
      conflictPolicy: "replace-with-backup",
      destinationPath: directory.canonicalPath,
      localGrantIds: ["grant-1"]
    })));
  } finally {
    Object.defineProperty(document, "elementFromPoint", { configurable: true, value: originalElementFromPoint });
  }
});

test("starting a new upload batch replaces older drawer rows even while they are still active", async () => {
  const firstRunning = sampleTransfer("running", {
    transferId: "transfer-first",
    sourceLabel: "first-upload.txt"
  });
  const nextRunning = sampleTransfer("running", {
    transferId: "transfer-next",
    sourceLabel: "next-upload.txt"
  });
  const stop = () => undefined;
  const selectUploadSources = vi.fn()
    .mockResolvedValueOnce([
      { grantId: "grant-first", displayName: "first-upload.txt", kind: "file", expiresAt: "2026-08-03T08:05:00.000Z" }
    ])
    .mockResolvedValueOnce([
      { grantId: "grant-next", displayName: "next-upload.txt", kind: "file", expiresAt: "2026-08-03T08:05:00.000Z" }
    ]);
  const enqueueTransfers = vi.fn()
    .mockResolvedValueOnce([firstRunning])
    .mockResolvedValueOnce([nextRunning]);
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    selectUploadSources,
    enqueueTransfers,
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDragState: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const stableProps = {
    activeTerminal: null,
    api,
    copy: workspaceCopy.en,
    cwd: null,
    followCwd: false,
    hosts: [host],
    isActive: true,
    selectedHostAlias: host.hostAlias,
    onError: vi.fn(),
    onFollowCwdChange: vi.fn(),
    onHostSelected: vi.fn(),
    onOpenTerminalAt: vi.fn(),
    onRecoveryCreated: vi.fn(),
    onViewRecoveries: vi.fn(),
    onTransfersQueued: vi.fn()
  };
  const view = render(<FilesPanel {...stableProps} transfers={[]} />);
  await screen.findByDisplayValue(session.homePath);

  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.upload }));
  await waitFor(() => expect(enqueueTransfers).toHaveBeenCalledTimes(1));
  view.rerender(<FilesPanel {...stableProps} transfers={[firstRunning]} />);
  expect(await screen.findByText("first-upload.txt")).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.upload }));
  await waitFor(() => expect(enqueueTransfers).toHaveBeenCalledTimes(2));
  view.rerender(<FilesPanel {...stableProps} transfers={[firstRunning, nextRunning]} />);
  expect(await screen.findByText("next-upload.txt")).toBeInTheDocument();
  expect(screen.queryByText("first-upload.txt")).not.toBeInTheDocument();
});

test("upload drawer stays on the current directory and clears ten seconds after completion", async () => {
  const running = sampleTransfer("running");
  const completed = sampleTransfer("completed", { targetLabel: `${session.homePath}/notes.txt` });
  const listDirectory = vi.fn().mockResolvedValue(page(session.homePath, [directory]));
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory,
    selectUploadSources: vi.fn().mockResolvedValue([
      { grantId: "grant-1", displayName: "notes.txt", kind: "file", expiresAt: "2026-08-03T08:05:00.000Z" }
    ]),
    enqueueTransfers: vi.fn().mockResolvedValue([running]),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDragState: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const stableProps = {
    activeTerminal: null,
    api,
    copy: workspaceCopy.en,
    cwd: null,
    followCwd: false,
    hosts: [host],
    isActive: true,
    selectedHostAlias: host.hostAlias,
    onError: vi.fn(),
    onFollowCwdChange: vi.fn(),
    onHostSelected: vi.fn(),
    onOpenTerminalAt: vi.fn(),
    onRecoveryCreated: vi.fn(),
    onViewRecoveries: vi.fn(),
    onTransfersQueued: vi.fn()
  };
  const view = render(<FilesPanel {...stableProps} transfers={[]} />);
  await screen.findByDisplayValue(session.homePath);
  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.upload }));
  await waitFor(() => expect(api.enqueueTransfers).toHaveBeenCalledWith(expect.objectContaining({
    conflictPolicy: "replace-with-backup"
  })));

  const staleConflict = sampleTransfer("waiting-conflict", {
    transferId: "transfer-stale-conflict",
    sourceLabel: "past-upload.txt"
  });
  view.rerender(<FilesPanel {...stableProps} transfers={[staleConflict, running]} />);
  expect(await screen.findByRole("region", { name: filesUiCopy.en.transferQueue })).toBeInTheDocument();
  expect(screen.getByText("notes.txt")).toBeInTheDocument();
  expect(screen.getByText("25 B/s")).toBeInTheDocument();
  expect(screen.queryByText("past-upload.txt")).not.toBeInTheDocument();

  const transferDrawer = screen.getByRole("region", { name: filesUiCopy.en.transferQueue });
  const tablePane = document.querySelector<HTMLElement>(".workspaceFilesTablePane");
  expect(tablePane).not.toBeNull();
  if (!tablePane) throw new Error("Files table pane is unavailable");
  Object.defineProperty(tablePane, "clientHeight", { configurable: true, value: 600 });
  vi.spyOn(transferDrawer, "getBoundingClientRect").mockReturnValue({
    bottom: 600,
    height: 210,
    left: 0,
    right: 800,
    top: 390,
    width: 800,
    x: 0,
    y: 390,
    toJSON: () => ({})
  });
  const transferResizeHandle = screen.getByRole("separator", { name: filesUiCopy.en.resizeTransfers });
  fireEvent.pointerDown(transferResizeHandle, { button: 0, clientY: 500 });
  fireEvent.pointerMove(window, { clientY: 400 });
  expect(transferDrawer.style.getPropertyValue("--workspace-file-transfer-height")).toBe("310px");
  fireEvent.pointerUp(window);

  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.collapseTransfers }));
  expect(screen.getByRole("region", { name: filesUiCopy.en.transferQueue })).toBeInTheDocument();
  expect(screen.queryByText("notes.txt")).not.toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.expandTransfers }));
  expect(screen.getByText("notes.txt")).toBeInTheDocument();

  vi.useFakeTimers();
  try {
    view.rerender(<FilesPanel {...stableProps} transfers={[completed]} />);
    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });
    expect(listDirectory).toHaveBeenCalledTimes(2);
    expect(listDirectory).toHaveBeenLastCalledWith(expect.objectContaining({ path: session.homePath }));
    expect(screen.getByDisplayValue(session.homePath)).toBeInTheDocument();
    expect(screen.getByRole("region", { name: filesUiCopy.en.transferQueue })).toBeInTheDocument();
    expect(screen.getByText(/1 completed in this batch/)).toBeInTheDocument();
    expect(screen.queryByText(workspaceCopy.en.loading)).not.toBeInTheDocument();

    act(() => vi.advanceTimersByTime(FILE_TRANSFER_COMPLETED_RETENTION_MS - 1));
    expect(screen.getByRole("region", { name: filesUiCopy.en.transferQueue })).toBeInTheDocument();

    act(() => vi.advanceTimersByTime(1));
    expect(screen.queryByRole("region", { name: filesUiCopy.en.transferQueue })).not.toBeInTheDocument();
  } finally {
    vi.useRealTimers();
  }
});

test("collapsing a completed upload drawer clears the current batch immediately", async () => {
  const running = sampleTransfer("running");
  const completed = sampleTransfer("completed");
  const stop = () => undefined;
  const api = {
    openFiles: vi.fn().mockResolvedValue(session),
    listDirectory: vi.fn().mockResolvedValue(page(session.homePath, [directory])),
    selectUploadSources: vi.fn().mockResolvedValue([
      { grantId: "grant-1", displayName: "notes.txt", kind: "file", expiresAt: "2026-08-03T08:05:00.000Z" }
    ]),
    enqueueTransfers: vi.fn().mockResolvedValue([running]),
    events: {
      onFileSearchUpdated: vi.fn().mockReturnValue(stop),
      onLocalDragState: vi.fn().mockReturnValue(stop),
      onLocalDrop: vi.fn().mockReturnValue(stop)
    }
  } as unknown as WorkspaceApi;
  const stableProps = {
    activeTerminal: null,
    api,
    copy: workspaceCopy.en,
    cwd: null,
    followCwd: false,
    hosts: [host],
    isActive: true,
    selectedHostAlias: host.hostAlias,
    onError: vi.fn(),
    onFollowCwdChange: vi.fn(),
    onHostSelected: vi.fn(),
    onOpenTerminalAt: vi.fn(),
    onRecoveryCreated: vi.fn(),
    onViewRecoveries: vi.fn(),
    onTransfersQueued: vi.fn()
  };
  const view = render(<FilesPanel {...stableProps} transfers={[]} />);
  await screen.findByDisplayValue(session.homePath);
  fireEvent.click(screen.getByRole("button", { name: workspaceCopy.en.upload }));
  await waitFor(() => expect(api.enqueueTransfers).toHaveBeenCalled());

  view.rerender(<FilesPanel {...stableProps} transfers={[running]} />);
  expect(await screen.findByRole("region", { name: filesUiCopy.en.transferQueue })).toBeInTheDocument();
  view.rerender(<FilesPanel {...stableProps} transfers={[completed]} />);
  expect(await screen.findByText(/1 completed in this batch/)).toBeInTheDocument();

  fireEvent.click(screen.getByRole("button", { name: filesUiCopy.en.collapseTransfers }));
  expect(screen.queryByRole("region", { name: filesUiCopy.en.transferQueue })).not.toBeInTheDocument();
});
