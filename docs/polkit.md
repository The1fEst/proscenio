# The polkit agent

Source: `modules/ii/polkit/Polkit.qml`, `PolkitContent.qml`,
`modules/common/widgets/FullscreenPolkitWindow.qml` and
`services/PolkitService.qml`, over Quickshell's `Quickshell.Services.Polkit`.

## 1. The agent

`PolkitAgent` registers the shell with polkitd as the authentication agent
for its session. When a program asks for an action that needs a password,
polkitd calls the agent with a message ("Authentication is needed to run
`/usr/bin/true` as the super user."), a cookie and the identities that may
answer. The agent runs polkit's helper with the cookie; the helper talks to
PAM and relays its questions. `PolkitService` keeps:

- `interactionAvailable`, true when a request starts or an attempt fails, and
  false from the moment a response is submitted;
- `cleanMessage`, the message without its final full stop;
- `cleanPrompt`, PAM's prompt trimmed and without a trailing colon, or
  "Password" (hidden response) or "Input" (visible response) when PAM sends
  none.

A wrong password starts the helper over with the same cookie, so the dialog
stays up until the user succeeds or cancels.

## 2. The window

Built only while a request is active: one `PanelWindow` per screen,
`WlrLayer.Overlay`, anchored to every edge, exclusion ignored, namespace
`quickshell:polkit`. The window on `Hyprland.focusedMonitor` takes the
keyboard `Exclusive`, the others `None`. Hyprland's rules give that namespace
`no_anim`.

Each window holds a `colScrim` rectangle fading in over `elementMoveFast`,
and a `WindowDialog` 450 px wide, centered:

- the `security` symbol, 26 px, `colSecondary`, centered;
- `WindowDialogTitle` "Authentication", centered;
- `WindowDialogParagraph` with `cleanMessage`, left-aligned and wrapped;
- a `MaterialTextField` filling the width, enabled while
  `interactionAvailable`, `cleanPrompt` as placeholder, password dots unless
  the response is meant to be visible;
- a button row with a 10 px bottom margin: a spacer, Cancel and OK. OK is
  enabled with the field.

Enter in the field and OK submit the text; Escape and Cancel cancel the
request, which polkitd reports to the program as a dismissal. Each time
interaction becomes available the field is cleared and focused.

---

**Status (proscenio).** Done. `src/services/polkit.rs` is the agent: it
exports `org.freedesktop.PolicyKit1.AuthenticationAgent` on the system bus
and registers for the logind session proscenio runs in. It reaches the helper
through `/run/polkit/agent-helper.socket` when polkit provides it, otherwise it
runs `polkit-agent-helper-1` itself. `src/panels/polkit.rs` is the window,
namespace `proscenio:polkit`.

As in qs: the window on Hyprland's focused monitor takes the keyboard
exclusively and the others take none; the field is focused as soon as PAM
asks; all screens share one field's text, so what is typed shows on every
screen and OK on any of them submits it; the windows exist only while a
request is active, and the typed text is cleared when the request ends.
