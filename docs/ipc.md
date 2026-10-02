# IPC

The QML shell answers `qs -c ii ipc call <target> <function> [arguments…]`
through the `IpcHandler` objects spread over its modules, and lists them with
`qs -c ii ipc show`. The dots use it for the brightness and media keys, and
`shellIsAlive` calls the made-up `TEST_ALIVE` target only to learn whether a
shell is running.

The qs command's exit status is 0 whenever an instance received the message,
even when the target or the function does not exist or the arguments do not
fit; the complaint is printed instead. It is 255 when no instance runs. The
messages are `Target not found.`, `Function not found.`,
`Function required to send message.`, and `Too many` / `Too few arguments
provided (N required but M were provided.)` followed by the function's
definition. `show` prints each target as `target <name>` and its functions as
`  function <name>(<parameters>): <type>`.

## Targets

| Target | Functions |
| --- | --- |
| `bar` | `toggle` `close` `open` |
| `sidebarRight` | `toggle` `close` `open` |
| `session` | `toggle` `close` `open` |
| `calendar` | `toggle` `close` `open` |
| `cheatsheet` | `toggle` `close` `open` |
| `mediaControls` | `toggle` `close` `open` |
| `osdVolume` | `trigger` `hide` `toggle` |
| `search` | `toggle` `workspacesToggle` `close` `open` `toggleReleaseInterrupt` `clipboardToggle` |
| `region` | `screenshot` `record` `recordWithSound` |
| `mpris` | `pauseAll` `playPause` `previous` `next` |
| `brightness` | `increment` `decrement` |
| `theme` | `toggleLightDark` |
| `cliphistService` | `update` |
| `wallpaperSelector` | `toggle` `random` |
| `wallpapers` | `apply(path: string)` |
| `settings` | `open` `openPage(page: string)` `close` `toggle` |
| `lock` | `activate` `focus` |
| `osk` | `toggle` `open` `close` |
| `welcome` | `open` `close` `toggle` |
| `renderer` | `keep` `revert` `reset` — proscenio only, see below |

`renderer keep` confirms a renderer on trial, `revert` goes back to the one
before it, and `reset` returns to Cairo. The last two restart the shell. `reset`
is the way out from a terminal when a renderer leaves the screen unusable.

`settings openPage` takes a page's own name: `quick`, `wifi`, `network`,
`bluetooth`, `displays`, `sound`, `power`, `multitasking`, `appearance`, `apps`,
`notifications`, `search`, `mouse`, `keyboard`, `accessibility`, `privacy`,
`system`.

`mpris next` skips to the end of the track when the player cannot go to the
next one but can seek.

---

**Status (proscenio).** `src/platform/ipc.rs`. `proscenio ipc show` and
`proscenio ipc call <target> <function> [arguments…]` take the same words,
print the same messages and return the same exit statuses. The client does
not start GTK: it calls the running shell over D-Bus and prints the reply.

The running shell exports `dev.fEst.Proscenio.Ipc` at `/dev/fEst/Proscenio`
on its application bus name `dev.fEst.Proscenio`, with two methods, callable
from anything that speaks D-Bus:

- `Show() → s targets`
- `Call(as arguments) → (s output, s error)`, `arguments` being what follows
  `call` on the command line.

Every target and function in the table is there except
`search toggleReleaseInterrupt`, whose `searchToggleRelease` shortcut is not
wired. Panel targets act on the focused monitor, as the global shortcuts do.
`search` also has `emojiToggle` and `workspacesClose`, which qs offers only
as global shortcuts. `osdVolume toggle` shows the indicator until the next
toggle, as setting `osdVolumeOpen` does in qs.

A second `proscenio` started while one runs exits without building a second
shell.

The dots' `keybinds.lua` goes through `shellIpc(...)`, which tries qs and then
proscenio, and `shellIsAlive` is `TEST_ALIVE` through it. Hyprland finds
`proscenio` on its `PATH`; `~/.local/bin` is on it.
