import { useMemo } from "react";
import type { CSSProperties } from "react";
import type { RemoteFileEntry, WorkspaceFilesSession } from "../types";
import type { FilesUiCopy } from "./filesUiCopy";
import { FileGlyph } from "./FileGlyph";
import { FilesIcon } from "./FilesIcon";

type TreeNode = { name: string; path: string; hidden?: boolean; virtualRoot?: boolean };

const LOCAL_ROOTS_PATH = "codexhub://local-roots";

function pathSegments(path: string) {
  const drive = path.match(/^([A-Za-z]:)\/?(.*)$/);
  if (drive) {
    const root = `${drive[1]}/`;
    const nodes: TreeNode[] = [{ name: drive[1], path: root }];
    let cursor = root.replace(/\/$/, "");
    for (const segment of drive[2].split("/").filter(Boolean)) {
      cursor += `/${segment}`;
      nodes.push({ name: segment, path: cursor, hidden: segment.startsWith(".") && segment.length > 1 });
    }
    return nodes;
  }
  const normalized = path === "/" ? "/" : `/${path.split("/").filter(Boolean).join("/")}`;
  const nodes: TreeNode[] = [{ name: "/", path: "/" }];
  if (normalized === "/") return nodes;
  let cursor = "";
  for (const segment of normalized.slice(1).split("/")) {
    cursor += `/${segment}`;
      nodes.push({ name: segment, path: cursor, hidden: segment.startsWith(".") && segment.length > 1 });
  }
  return nodes;
}

function quickAccessLabel(path: string, copy: FilesUiCopy) {
  if (path === "/") return copy.rootDirectory;
  if (/^[A-Za-z]:\/$/.test(path)) return path.slice(0, 2);
  const normalized = path.replace(/\/+$/, "");
  const segments = normalized.split("/").filter(Boolean);
  return segments[segments.length - 1] ?? path;
}

function buildTreeChildren(
  session: WorkspaceFilesSession,
  currentPath: string | null,
  directoryEntriesByPath: ReadonlyMap<string, RemoteFileEntry[]>,
  localRoots: readonly string[]
) {
  const children = new Map<string, TreeNode[]>();
  const mergeChild = (parent: string, node: TreeNode) => {
    const current = children.get(parent) ?? [];
    if (!current.some((candidate) => candidate.path === node.path)) {
      children.set(parent, [...current, node].sort((a, b) => a.name.localeCompare(b.name)));
    }
  };

  if (session.targetKind === "local" && localRoots.some((path) => /^[A-Za-z]:\/$/.test(path))) {
    children.set(LOCAL_ROOTS_PATH, localRoots.map((path) => ({ name: path.slice(0, 2), path })));
  }

  // Session and canonical page paths are authoritative, so their ancestry can
  // safely keep the active location visible before a parent is lazily listed.
  for (const path of [session.homePath, currentPath].filter((value): value is string => Boolean(value))) {
    const chain = pathSegments(path);
    for (let index = 1; index < chain.length; index += 1) {
      mergeChild(chain[index - 1].path, chain[index]);
    }
  }
  for (const [path, entries] of directoryEntriesByPath) {
    for (const entry of entries) {
      mergeChild(path, {
        name: entry.name,
        path: entry.canonicalPath,
        hidden: entry.name.startsWith(".") && entry.name.length > 1
      });
    }
  }
  return children;
}

function TreeBranch({
  copy,
  currentPath,
  depth,
  expandedPaths,
  loadingPaths,
  node,
  treeChildren,
  onContextMenu,
  onNavigate,
  onToggle
}: {
  copy: FilesUiCopy;
  currentPath: string | null;
  depth: number;
  expandedPaths: ReadonlySet<string>;
  loadingPaths: ReadonlySet<string>;
  node: TreeNode;
  treeChildren: ReadonlyMap<string, TreeNode[]>;
  onContextMenu: (path: string, point: { x: number; y: number }) => void;
  onNavigate: (path: string) => void;
  onToggle: (path: string) => void;
}) {
  const expanded = node.virtualRoot || expandedPaths.has(node.path);
  const children = treeChildren.get(node.path) ?? [];
  const label = node.path === "/" ? copy.rootDirectory : node.name;
  return (
    <li role="treeitem" aria-expanded={expanded} aria-current={currentPath === node.path ? "page" : undefined}>
      <div
        className="workspaceDirectoryTreeRow"
        data-active={currentPath === node.path}
        data-hidden={node.hidden ? "true" : undefined}
        style={{ "--tree-depth": depth } as CSSProperties}
        onContextMenu={(event) => {
          if (node.virtualRoot) return;
          event.preventDefault();
          event.stopPropagation();
          onContextMenu(node.path, { x: event.clientX, y: event.clientY });
        }}
      >
        {node.virtualRoot ? (
          <span aria-hidden="true" className="workspaceDirectoryChevron"><FilesIcon name="chevronDown" /></span>
        ) : (
          <button
            aria-label={`${expanded ? copy.collapseDirectory : copy.expandDirectory}: ${label}`}
            className="workspaceDirectoryChevron"
            data-loading={loadingPaths.has(node.path)}
            type="button"
            onClick={() => onToggle(node.path)}
          >{loadingPaths.has(node.path) ? "·" : <FilesIcon name={expanded ? "chevronDown" : "chevronRight"} />}</button>
        )}
        {node.virtualRoot ? (
          <span className="workspaceDirectoryName"><FileGlyph kind="directory" /><span>{label}</span></span>
        ) : (
          <button className="workspaceDirectoryName" title={node.path} type="button" onClick={() => onNavigate(node.path)}>
            <FileGlyph kind="directory" />
            <span>{label}</span>
          </button>
        )}
      </div>
      {expanded && children.length > 0 ? (
        <ul role="group">
          {children.map((child) => (
            <TreeBranch
              copy={copy}
              currentPath={currentPath}
              depth={depth + 1}
              expandedPaths={expandedPaths}
              key={child.path}
              loadingPaths={loadingPaths}
              node={child}
              treeChildren={treeChildren}
              onContextMenu={onContextMenu}
              onNavigate={onNavigate}
              onToggle={onToggle}
            />
          ))}
        </ul>
      ) : null}
    </li>
  );
}

export function DirectoryTree({
  copy,
  currentPath,
  directoryEntriesByPath,
  expandedPaths,
  localRoots,
  loadingPaths,
  quickAccessPaths,
  session,
  onContextMenu,
  onNavigate,
  onQuickAccessContextMenu,
  onToggle
}: {
  copy: FilesUiCopy;
  currentPath: string | null;
  directoryEntriesByPath: ReadonlyMap<string, RemoteFileEntry[]>;
  expandedPaths: ReadonlySet<string>;
  localRoots: readonly string[];
  loadingPaths: ReadonlySet<string>;
  quickAccessPaths: readonly string[];
  session: WorkspaceFilesSession | null;
  onContextMenu: (path: string, point: { x: number; y: number }) => void;
  onNavigate: (path: string) => void;
  onQuickAccessContextMenu: (path: string, point: { x: number; y: number }) => void;
  onToggle: (path: string) => void;
}) {
  const treeChildren = useMemo(
    () => session ? buildTreeChildren(session, currentPath, directoryEntriesByPath, localRoots) : new Map<string, TreeNode[]>(),
    [currentPath, directoryEntriesByPath, localRoots, session]
  );
  const hasWindowsRoots = session?.targetKind === "local" && localRoots.some((path) => /^[A-Za-z]:\/$/.test(path));
  const rootNode = hasWindowsRoots
    ? { name: copy.computerRoot, path: LOCAL_ROOTS_PATH, virtualRoot: true }
    : session ? pathSegments(currentPath ?? session.homePath)[0] ?? { name: "/", path: "/" } : { name: "/", path: "/" };

  return (
    <aside aria-label={copy.directoryTree} className="workspaceFilesTree">
      <header className="workspaceFilesSideHeader"><strong>{copy.directoryTree}</strong></header>
      <div className="workspaceFilesTreeScroll">
        {session ? (
          <ul aria-label={copy.directoryTree} className="workspaceDirectoryTree" role="tree">
            <TreeBranch
              copy={copy}
              currentPath={currentPath}
              depth={0}
              expandedPaths={expandedPaths}
              loadingPaths={loadingPaths}
              node={rootNode}
              treeChildren={treeChildren}
              onContextMenu={onContextMenu}
              onNavigate={onNavigate}
              onToggle={onToggle}
            />
          </ul>
        ) : null}
      </div>
      {/* Tree nodes scroll independently so quick access stays anchored to the card footer. */}
      {session ? (
        <section className="workspaceFilesQuickAccess">
          <h3>{copy.quickAccess}</h3>
          <button type="button" onClick={() => onNavigate(session.homePath)}><FilesIcon name="home" />{copy.homeDirectory}</button>
          {currentPath ? <button type="button" onClick={() => onNavigate(currentPath)}><FilesIcon name="current" />{copy.currentDirectory}</button> : null}
          {quickAccessPaths.map((path) => (
            <button
              data-quick-access-path={path}
              key={path}
              title={path}
              type="button"
              onClick={() => onNavigate(path)}
              onContextMenu={(event) => {
                event.preventDefault();
                event.stopPropagation();
                onQuickAccessContextMenu(path, { x: event.clientX, y: event.clientY });
              }}
            ><FileGlyph kind="directory" /><span>{quickAccessLabel(path, copy)}</span></button>
          ))}
        </section>
      ) : null}
    </aside>
  );
}
