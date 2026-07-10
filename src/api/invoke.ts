import { invoke, isTauri } from "@tauri-apps/api/core";
import type { ApiError } from "../generated/rust-contracts";
import type { TauriCommand } from "./commands";

export type DesktopCommandErrorKind = "backend-unavailable" | "invalid-arguments" | "invoke-failed";

export class DesktopCommandError extends Error {
  readonly command: TauriCommand;
  readonly kind: DesktopCommandErrorKind;
  readonly apiError: ApiError | null;

  constructor(command: TauriCommand, kind: DesktopCommandErrorKind, message: string, apiError: ApiError | null = null) {
    super(message);
    this.name = kind === "backend-unavailable" ? "DesktopBackendUnavailableError" : "DesktopCommandError";
    this.command = command;
    this.kind = kind;
    this.apiError = apiError;
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

export function requireHostAlias(command: TauriCommand, value: string | null | undefined) {
  const alias = value?.trim() ?? "";
  if (!alias) {
    throw new DesktopCommandError(command, "invalid-arguments", `${command}: A non-empty SSH host alias is required.`);
  }
  return alias;
}

export async function requiredInvoke<T>(command: TauriCommand, args?: Record<string, unknown>): Promise<T> {
  assertTauriRuntime(command);
  try {
    return await invoke<T>(command, args);
  } catch (error) {
    const apiError = parseApiError(error);
    throw new DesktopCommandError(command, "invoke-failed", formatInvokeError(error, command), apiError);
  }
}

export function formatInvokeError(error: unknown, command?: TauriCommand) {
  const structured = parseApiError(error);
  const detail = structured?.message
    ?? (typeof error === "string" ? error : error instanceof Error ? error.message : "Desktop backend command failed.");
  const redacted = redactSensitiveText(detail);
  return command ? `${command}: ${redacted}` : redacted;
}

export function parseApiError(error: unknown): ApiError | null {
  if (error instanceof DesktopCommandError) return error.apiError;
  if (isApiError(error)) return error;
  if (typeof error !== "string") return null;
  const taskEnvelope = parseTaskErrorEnvelope(error);
  if (taskEnvelope) return taskEnvelope;
  try {
    const parsed: unknown = JSON.parse(error);
    return isApiError(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function parseTaskErrorEnvelope(value: string): ApiError | null {
  const match = /^task-error:([a-zA-Z0-9-]+):(.*)$/su.exec(value);
  if (!match) return null;
  return {
    code: "operation-failed",
    message: match[2].trim(),
    retryable: true,
    taskId: match[1],
    recoveryId: null
  };
}

function isApiError(value: unknown): value is ApiError {
  if (!value || typeof value !== "object") return false;
  const candidate = value as Partial<ApiError>;
  return typeof candidate.code === "string"
    && typeof candidate.message === "string"
    && typeof candidate.retryable === "boolean";
}

function redactSensitiveText(value: string) {
  return value
    .replace(/-----BEGIN[\s\S]*?PRIVATE KEY-----[\s\S]*?-----END[\s\S]*?PRIVATE KEY-----/gi, "[REDACTED PRIVATE KEY]")
    .replace(/\bsk-[A-Za-z0-9_-]{8,}\b/g, "[REDACTED API KEY]")
    .replace(/\bBearer\s+[^\s,;]+/gi, "Bearer [REDACTED]")
    .replace(/\b(password|passphrase|token|api[_ -]?key)\s*[:=]\s*[^\s,;]+/gi, "$1=[REDACTED]");
}
