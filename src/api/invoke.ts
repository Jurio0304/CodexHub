import { invoke } from "@tauri-apps/api/core";

type InvokeFallback<T> = T | (() => T | Promise<T>);

export async function safeInvoke<T>(
  command: string,
  args: Record<string, unknown> | undefined,
  fallback: InvokeFallback<T>
): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch {
    return typeof fallback === "function" ? await (fallback as () => T | Promise<T>)() : fallback;
  }
}

export async function requiredInvoke<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new Error(formatInvokeError(error));
  }
}

export function hasTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

export function formatInvokeError(error: unknown) {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "The Tauri desktop backend is required for this operation.";
}
