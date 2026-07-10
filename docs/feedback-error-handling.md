# Feedback, Errors, And Accessible Dialogs

Date: 2026-07-10

CodexHub uses one feedback path so a completed action is visible, keyboard-accessible, and linked to durable evidence when the action can change user or remote state.

## Feedback Lifetime

| Event | UI | Persistent task |
| --- | --- | --- |
| Form validation, selection, copy confirmation, pure UI action | Inline status or five-second Toast | No |
| Successful read refresh | Five-second Toast when user-triggered | No |
| Read failure before external work starts | Dismissible persistent error with retry where available | No |
| Settings/Host/Profile/Skill durable write | Success Toast or persistent error | Always |
| Live SSH, probe, install, update, apply, sync | Progress surface plus Task drawer | Queued, running, and final transitions |
| Partial failure, interrupted work, migration/recovery failure | Persistent error with task/recovery route | Always |
| React render failure | Root Error Boundary with sanitized fixed text | Frontend-failure task when SQLite is available |

`FeedbackProvider` deduplicates by tone, task id, and message. Success/info Toasts last five seconds, warnings last eight seconds, and errors never auto-dismiss. A task-linked error opens the durable task and acknowledges it only after the user views the task.

## Task Surfaces

- The global Task drawer shows running and unacknowledged failed/interrupted work first, followed by the 20 latest tasks.
- The Tasks page uses cursor pagination for complete SQLite history.
- `task-updated` is a notification event. Event-delivery failure is logged with redaction; SQLite remains authoritative.
- SSH bootstrap and remote Codex progress messages are redacted and appended to SQLite before completion. Final task persistence merges them instead of replacing the running log.
- Sidebar success indicators clear when the owning page is entered. Failure indicators come from unacknowledged tasks and cannot be cleared by scrolling, pointer movement, or unrelated keyboard input.

## Error Contract

Structured commands return `ApiError { code, message, retryable, taskId, recoveryId }`. Legacy string-returning commands use a sanitized `task-error:<taskId>:` compatibility envelope after a durable failure, so the same persistent error action can open its task. The title/action is localized from `code`; `message` is a redacted technical summary. Desktop IPC or storage failure never calls the Mock API. Mock mode is selected explicitly at build/start time.

Error Boundary reports use a fixed `React render failure.` payload. Raw exception text, component stacks, credentials, tokens, passwords, and private key material are neither rendered nor persisted.

## Dialog Contract

Shared Dialog and AlertDialog wrappers provide:

- first editable field focus for forms;
- Cancel focus for destructive confirmation;
- Tab/Shift+Tab containment in the top-level dialog;
- Esc through the same close callback;
- blocked Esc/backdrop close while a write is busy;
- focus restoration to the triggering control, falling back to active navigation;
- `aria-labelledby`/`aria-describedby`, scoped `role="status"`, and persistent `role="alert"` regions.

`prefers-reduced-motion: reduce` disables Toast/drawer movement, spinners, and existing transitions while retaining visible text and static busy state.

## Validation

`pnpm test:ui` covers Toast lifetime, persistent task actions, localization, Error Boundary redaction, Task drawer focus, Dialog/AlertDialog focus behavior, and axe checks. `pnpm test:i18n` rejects Chinese UI literals outside copy registries and verifies identical English/Chinese keys.
