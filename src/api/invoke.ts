import { invoke, isTauri } from "@tauri-apps/api/core";
import type { TauriCommand } from "./commands";

export type DesktopCommandErrorKind = "backend-unavailable" | "invoke-failed";

export class DesktopCommandError extends Error {
  readonly command: TauriCommand;
  readonly kind: DesktopCommandErrorKind;

  constructor(command: TauriCommand, kind: DesktopCommandErrorKind, message: string) {
    super(message);
    this.name = kind === "backend-unavailable" ? "DesktopBackendUnavailableError" : "DesktopCommandError";
    this.command = command;
    this.kind = kind;
  }
}

export function hasTauriRuntime() {
  if (!isTauri()) return false;
  const internals = (globalThis as typeof globalThis & {
    __TAURI_INTERNALS__?: { invoke?: unknown };
  }).__TAURI_INTERNALS__;
  return typeof internals?.invoke === "function";
}

export function assertTauriRuntime(command: TauriCommand) {
  if (!hasTauriRuntime()) {
    throw new DesktopCommandError(
      command,
      "backend-unavailable",
      "The Tauri desktop backend is unavailable. Start CodexHub Desktop or explicitly use Mock mode."
    );
  }
}

export async function requiredInvoke<T>(command: TauriCommand, args?: Record<string, unknown>): Promise<T> {
  assertTauriRuntime(command);
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    throw new DesktopCommandError(command, "invoke-failed", formatInvokeError(error, command));
  }
}

export function formatInvokeError(error: unknown, command?: TauriCommand) {
  const detail = typeof error === "string" ? error : error instanceof Error ? error.message : "Desktop backend command failed.";
  const redacted = redactSensitiveText(detail);
  return command ? `${command}: ${redacted}` : redacted;
}

function redactSensitiveText(value: string) {
  return value
    .replace(/-----BEGIN[\s\S]*?PRIVATE KEY-----[\s\S]*?-----END[\s\S]*?PRIVATE KEY-----/gi, "[REDACTED PRIVATE KEY]")
    .replace(/\bsk-[A-Za-z0-9_-]{8,}\b/g, "[REDACTED API KEY]")
    .replace(/\bBearer\s+[^\s,;]+/gi, "Bearer [REDACTED]")
    .replace(/\b(password|passphrase|token|api[_ -]?key)\s*[:=]\s*[^\s,;]+/gi, "$1=[REDACTED]");
}
