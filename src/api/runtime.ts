/// <reference types="vite/client" />

export type ApiMode = "desktop" | "mock";

// Mock mode is a build-time choice. A missing desktop bridge never selects it.
export function resolveApiMode(mode: string | undefined): ApiMode {
  return mode === "mock" ? "mock" : "desktop";
}

// Keep the env access direct so Vite can replace MODE in mock and desktop builds.
export const apiMode: ApiMode = resolveApiMode(import.meta.env.MODE);
