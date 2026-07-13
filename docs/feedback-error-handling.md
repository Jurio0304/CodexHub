# Feedback, Errors, And Accessible Dialogs

Date: 2026-07-10

CodexHub uses one feedback path so a completed action is visible, keyboard-accessible, and linked to durable evidence when the action can change user or remote state.

## Feedback Lifetime

| Event | UI | Persistent task |
| --- | --- | --- |
| Form validation, selection, copy confirmation, pure UI action | Inline status or five-second Toast | No |
| Successful read refresh | Five-second Toast when user-triggered | No |
| Read failure before external work starts | Five-second failure Toast with retry where available | No |
| Settings/Host/Profile/Skill durable write | Five-second success/failure Toast | Always |
| Live SSH, probe, install, update, apply, sync | Progress surface plus Tasks page record | Queued, running, and final transitions |
| Partial failure, interrupted work, migration/recovery failure | Five-second failure Toast with task/recovery route | Always |
| React render failure | Root Error Boundary with sanitized fixed text | Frontend-failure task when SQLite is available |

`FeedbackProvider` deduplicates by placement, tone, task id, and message. Every Toast starts closing within five seconds. Its one-second entrance moves a short distance upward while changing from blurred to sharp; its one-second exit reverses that motion. Pointer, keyboard, touch, wheel, or scroll input starts the exit immediately. There is no close icon; an optional task/retry action runs first and then closes the Toast.

The four semantic tones use theme-aware pale surfaces, stronger borders, and an elevated shadow: blue for information, yellow for warning, green for success, and red for failure. `detail` feedback is centered over the content pane, while `global` feedback is centered over the full app viewport. Host-test and Codex-maintenance completion Toasts switch to `global` while their live log modal is visible and return to `detail` when no such modal exists. Dialog-driven actions such as Add Server, New API config, and skill download may also request `global` explicitly.

The persisted `hostOperationLogPopups` preference controls only the automatic live log modal for single or batch host tests and Codex install, update, uninstall, or batch update. The modal header's **Don't show again** action opens a separate confirmation before saving the preference; the same pill switch appears immediately below **Sidebar visual hints** in Settings. Turning the preference off does not cancel, pause, or discard an operation, its redacted logs, or its completion Toast, and Settings can enable it again later.

## Task Surfaces

- The Tasks page reads the authoritative retained SQLite history and never displays more than 100 task records.
- The complete task list keeps at most the latest 100 task records. Automatic retention and the Tasks-page **Clear all** action first export complete task/log records as a JSON archive, move that file to the operating system recycle bin, and only then delete the completed SQLite rows. Running and queued tasks remain active.
- Resource-monitor sampling writes one task for the first page entry or a manual refresh. Scheduled auto-refresh remains taskless so polling cannot displace user-initiated history.
- Live progress and Tasks history share the same two-level disclosure. Every step card, including failed steps, starts collapsed. Opening a step reveals concise level-and-message rows; opening one of those rows then reveals command, exit code, duration, timeout state, stdout, and stderr.
- Disabling live log pop-ups never removes stored steps or logs. The Tasks page remains the authoritative retained history and can reopen the same step cards after the operation finishes.
- `task-updated` is a notification event. Event-delivery failure is logged with redaction; SQLite remains authoritative.
- SSH bootstrap and remote Codex progress messages are redacted and appended to SQLite before completion. Final task persistence merges them instead of replacing the running log.
- Sidebar success and failure indicators share one transient state and clear when the owning page is entered or receives the next interaction.

## Error Contract

Structured commands return `ApiError { code, message, retryable, taskId, recoveryId }`. Legacy string-returning commands use a sanitized `task-error:<taskId>:` compatibility envelope after a durable failure, so the same transient error action can open its task. Chinese feedback renders localized operation/recovery guidance while the redacted technical summary remains in task logs. Desktop IPC or storage failure never calls the Mock API. Mock mode is selected explicitly at build/start time.

Error Boundary reports use a fixed `React render failure.` payload. Raw exception text, component stacks, credentials, tokens, passwords, and private key material are neither rendered nor persisted.

## Dialog Contract

Shared Dialog and AlertDialog wrappers provide:

- first editable field focus for forms;
- Cancel focus for destructive confirmation;
- Tab/Shift+Tab containment in the top-level dialog;
- Esc through the same close callback;
- blocked Esc/backdrop close while a write is busy;
- focus restoration to the triggering control, falling back to active navigation;
- fixed, centered portal content above the modal overlay, including nested skill download/uninstall/delete confirmations;
- `aria-labelledby`/`aria-describedby`, scoped `role="status"`, and persistent `role="alert"` regions.

`prefers-reduced-motion: reduce` disables Toast/drawer movement, spinners, and existing transitions while retaining visible text and static busy state.

## Validation

`pnpm test:ui` covers five-second Toast lifetime, interaction dismissal, task actions, localization, Error Boundary redaction, Dialog/AlertDialog focus behavior, and axe checks. `pnpm test:i18n` rejects Chinese UI literals outside copy registries, verifies identical English/Chinese keys, and guards localized task actions/summaries.
