export type ApiMode = "desktop" | "mock";

type ImportMetaWithEnv = ImportMeta & {
  env?: {
    MODE?: string;
  };
};

// Mock mode is a build-time choice. A missing desktop bridge never selects it.
export function resolveApiMode(meta: ImportMetaWithEnv = import.meta as ImportMetaWithEnv): ApiMode {
  return meta.env?.MODE === "mock" ? "mock" : "desktop";
}

export const apiMode: ApiMode = resolveApiMode();
