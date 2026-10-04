---
title: Polkit agent
sidebar_label: Polkit agent
description: How proscenio acts as the session's polkit authentication agent, from registering with polkitd to the password dialog on every monitor.
---

# Polkit agent

When a program asks polkit for something that needs a password, such as `pkexec`, a mount, a
system setting or a package manager front end, polkit asks the session's authentication agent to
authenticate the user. proscenio is that agent: the question appears in the shell's own dialog, on
every monitor.

```bash
pkexec true
```

## Registration

At start, proscenio exports `org.freedesktop.PolicyKit1.AuthenticationAgent` at
`/dev/fEst/Proscenio/PolkitAgent` on the system bus, and calls `RegisterAuthenticationAgent` on
polkit's authority for its login session. The session is `XDG_SESSION_ID`, or else the logind session
the process belongs to; the locale passed along is `LANG`, or `en_US.UTF-8` without it.

polkit takes one agent per session. When another agent already holds it, the registration fails and
proscenio prints `polkit: the agent could not register: …` on stderr; the rest of the shell runs as
usual.

## A request

polkitd calls `BeginAuthentication` with a message, a cookie and the identities that may answer.
proscenio picks one user: the user it runs as when that user is among them, as a user or as a member
of an offered group, and otherwise the first user offered.

The password goes to polkit's helper, which checks it with PAM. proscenio connects to
`/run/polkit/agent-helper.socket` when polkit provides it, and otherwise runs
`/usr/lib/polkit-1/polkit-agent-helper-1` itself. The helper relays PAM's questions, and the dialog
asks each one in turn:

- the message, without its final full stop, explains what the program wants;
- the field's placeholder is PAM's prompt without its trailing colon; when PAM gives none, it is
  "Password", or "Input" for an answer that may be shown;
- the field and OK are disabled until PAM asks, and the typed text is hidden unless PAM asks for a
  visible answer.

Enter or OK sends the answer; Escape or Cancel cancels, and the program hears
`org.freedesktop.PolicyKit1.Error.Cancelled`. A wrong password starts the helper over with the same
cookie, so the dialog stays up with the field cleared and focused, until the password is right or the
request is cancelled. polkitd can cancel a request too, with `CancelAuthentication`, which closes the
dialog.

proscenio handles one request at a time. A second one that arrives while the dialog is up fails at
once with `Another authentication is in progress`.

## The dialog

The dialog exists only while a request is open. It is one layer-shell window per monitor, on the
overlay layer, anchored to every edge and covering the whole screen, with the namespace
`proscenio:polkit` for Hyprland's layer rules. The window on the monitor Hyprland reports as focused
takes the keyboard exclusively; the others take none. All windows share one field, so what is typed
shows on every screen, and OK on any of them sends it. The typed text is cleared when the request
ends.

## proscenio's own system settings

The settings that change the system run `proscenio` as root through `pkexec`, so this agent asks for
their password too. [Building and installing](installing.md) covers the polkit policy that lets one
authentication cover several changes, and [Command line](command-line.md) lists the commands.
