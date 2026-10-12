import type { ReactNode } from "react";

/** Small, shared line icons keep the file toolbar crisp at full and split widths. */
export type FilesIconName =
  | "back"
  | "chevronDown"
  | "chevronRight"
  | "chevronUp"
  | "close"
  | "copy"
  | "current"
  | "cut"
  | "download"
  | "eye"
  | "eyeOff"
  | "folderPlus"
  | "forward"
  | "home"
  | "locate"
  | "more"
  | "paste"
  | "refresh"
  | "search"
  | "sortAscending"
  | "sortDescending"
  | "tree"
  | "transfer"
  | "trash"
  | "up"
  | "upload";

const iconPaths: Record<FilesIconName, ReactNode> = {
  back: <path d="m9.5 3.5-4.5 4.5 4.5 4.5" />,
  chevronDown: <path d="m4.5 6 3.5 3.5L11.5 6" />,
  chevronRight: <path d="m6 4.5 3.5 3.5L6 11.5" />,
  chevronUp: <path d="m4.5 10 3.5-3.5 3.5 3.5" />,
  close: <path d="m4 4 8 8m0-8-8 8" />,
  copy: <><rect x="5" y="5" width="7" height="7" rx="1" /><path d="M3 10V3.8A.8.8 0 0 1 3.8 3H10" /></>,
  current: <><circle cx="8" cy="8" r="4" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2" /></>,
  cut: <><circle cx="4" cy="12" r="2" /><circle cx="12" cy="12" r="2" /><path d="m5.6 10.8 6.2-7.3M10.4 10.8 4.2 3.5" /></>,
  download: <><path d="M8 2.5v7" /><path d="m5 7 3 3 3-3" /><path d="M3 13.5h10" /></>,
  eye: <><path d="M1.5 8S3.8 4.5 8 4.5 14.5 8 14.5 8 12.2 11.5 8 11.5 1.5 8 1.5 8Z" /><circle cx="8" cy="8" r="1.8" /></>,
  eyeOff: <><path d="M2.1 2.1 13.9 13.9" /><path d="M4.1 4.4C2.6 5.5 1.5 8 1.5 8s2.3 3.5 6.5 3.5c1.2 0 2.2-.3 3.1-.8M6.1 4.7c.6-.2 1.2-.2 1.9-.2 4.2 0 6.5 3.5 6.5 3.5s-.8 1.3-2.1 2.3" /><path d="M6.7 6.7a1.8 1.8 0 0 0 2.6 2.6" /></>,
  folderPlus: <><path d="M2.5 4.8A1.3 1.3 0 0 1 3.8 3.5h3l1.2 1.3h4.2a1.3 1.3 0 0 1 1.3 1.3v5.6a1.3 1.3 0 0 1-1.3 1.3H3.8a1.3 1.3 0 0 1-1.3-1.3V4.8Z" /><path d="M10.5 7.3v3M9 8.8h3" /></>,
  forward: <path d="m6.5 3.5 4.5 4.5-4.5 4.5" />,
  home: <><path d="m2.5 7 5.5-4.5L13.5 7v6H2.5V7Z" /><path d="M6.2 13V9.2h3.6V13" /></>,
  locate: <><circle cx="8" cy="8" r="3" /><path d="M8 1.5v2M8 12.5v2M1.5 8h2M12.5 8h2" /></>,
  more: <><circle cx="4" cy="8" r=".7" fill="currentColor" stroke="none" /><circle cx="8" cy="8" r=".7" fill="currentColor" stroke="none" /><circle cx="12" cy="8" r=".7" fill="currentColor" stroke="none" /></>,
  paste: <><path d="M5 4h6v10H5z" /><path d="M7 2.5h2a1 1 0 0 1 1 1V4H6v-.5a1 1 0 0 1 1-1Z" /></>,
  refresh: <path d="M13 5.5V2.8l-1.6 1.6A5.4 5.4 0 1 0 13.2 10" />,
  search: <><circle cx="7" cy="7" r="4.2" /><path d="m10.2 10.2 3 3" /></>,
  sortAscending: <><path d="M3 4h7M3 8h5M3 12h3M13 12V4m0 0-2 2m2-2 2 2" /></>,
  sortDescending: <><path d="M3 4h3M3 8h5M3 12h7M13 4v8m0 0-2-2m2 2 2-2" /></>,
  tree: <><rect x="2.5" y="2.5" width="11" height="11" rx="1.5" /><path d="M6 2.5v11M8.5 5.5h2.5M8.5 8h2.5M8.5 10.5h2.5" /></>,
  transfer: <><path d="M3 5h9m0 0L9.5 2.5M12 5 9.5 7.5" /><path d="M13 11H4m0 0 2.5-2.5M4 11l2.5 2.5" /></>,
  trash: <><path d="M3.5 4.5h9M6 4.5V3h4v1.5M5 6.5l.5 6h5l.5-6M7 7.5v3.5M9 7.5v3.5" /></>,
  up: <><path d="M8 13V3" /><path d="m4.8 6.2L8 3l3.2 3.2" /></>,
  upload: <><path d="M8 13V6" /><path d="m5 8.8 3-3 3 3" /><path d="M3 13.5h10" /></>
};

export function FilesIcon({ name, size = 16 }: { name: FilesIconName; size?: number }) {
  return (
    <svg
      aria-hidden="true"
      className="workspaceFilesActionIcon"
      fill="none"
      height={size}
      viewBox="0 0 16 16"
      width={size}
    >
      <g stroke="currentColor" strokeLinecap="round" strokeLinejoin="round" strokeWidth="1.35">
        {iconPaths[name]}
      </g>
    </svg>
  );
}
