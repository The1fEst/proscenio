---
title: Keyring prompt
sidebar_label: Keyring prompt
description: How proscenio serves as the gcr system prompter for gnome-keyring and other gcr clients, the D-Bus protocol and secret exchange behind it, and how the lock screen answers keyring prompts.
---

# Keyring prompt

gnome-keyring does not draw its own dialogs. When it needs a password, to unlock a keyring or to
choose the password of one being created, it asks the system prompter on the session bus, and so do
other gcr clients such as `pinentry-gnome3`. proscenio is that prompter: the question appears in the
shell's own dialog, on every monitor.

## The prompter

At start, proscenio exports `org.gnome.keyring.internal.Prompter` at `/org/gnome/keyring/Prompter`
and takes the names `org.gnome.keyring.SystemPrompter` and `org.gnome.keyring.PrivatePrompter` on the
session bus. While it holds them, the bus starts no `gcr-prompter`.

A caller runs one prompt per callback object it exports, which implements
`org.gnome.keyring.internal.Prompter.Callback`:

1. `BeginPrompting(o callback)`. proscenio sets up a secret exchange and calls the callback's
   `PromptReady("", {}, exchange)` with its public key.
2. `PerformPrompt(o callback, s type, a{sv} properties, s exchange)`. `type` is `password` or
   `confirm`; the properties describe the prompt (see below); the exchange carries the caller's
   public key. The prompt then waits for the user.
3. The answer comes as `PromptReady(reply, changed, exchange)`. `reply` is `yes` or `no`. `changed`
   holds `choice-chosen` when the prompt offers a choice, and `password-strength` when it asks to
   choose a password: 0 for an empty one, 1 otherwise. For a password, the exchange carries it
   encrypted.
4. The caller may perform the prompt again, as gnome-keyring does after a wrong password with
   `warning` set, or end it with `StopPrompting(o callback)`, which proscenio answers with the
   callback's `PromptDone()`.

| Error | When |
|---|---|
| `org.gnome.keyring.Prompter.Failed` | `BeginPrompting` twice for the same callback; `PerformPrompt` or `StopPrompting` before `BeginPrompting`; an unknown `type`; an exchange that cannot be read |
| `org.gnome.keyring.Prompter.InProgress` | `PerformPrompt` while the same callback's prompt still waits |

A caller that leaves the bus loses its prompts.

### Properties

| Property | Use |
|---|---|
| `message` | The title |
| `title` | The title when there is no `message`; "Authentication" when there is neither |
| `description` | A paragraph under the title |
| `warning` | A paragraph in the error color, such as after a wrong password |
| `password-new` | Asks to choose a password: a second field repeats it |
| `choice-label` | Adds a switch with this label |
| `choice-chosen` | The switch's state |
| `continue-label`, `cancel-label` | The button labels, "OK" and "Cancel" when unset; mnemonic underscores are dropped |

Properties sent with a later `PerformPrompt` update those of the first.

## The secret exchange

The password never crosses the bus in clear. Both sides speak gcr's `sx-aes-1` exchange, a key file
of one group:

```ini
[sx-aes-1]
public=<base64>
secret=<base64>
iv=<base64>
```

`public` is a Diffie-Hellman public key in the 1536-bit MODP group of RFC 3526 with generator 2,
big-endian without leading zeros. The shared secret, padded to 192 bytes, goes through HKDF-SHA256
with no salt and no info to a 16-byte key. `secret` is the password encrypted with AES-128-CBC and
PKCS#7 padding under that key, and `iv` is its 16 random bytes. Both are left out when no secret
travels: in the first `PromptReady`, in the answer to a confirmation, and in an answer of `no`.

## The dialog

The dialog shows the first waiting prompt while the screen is unlocked; the others wait their turn.
It is one layer-shell window per monitor, on the overlay layer and covering the whole screen, with
the namespace `proscenio:keyring` for Hyprland's layer rules. The window on the monitor Hyprland
reports as focused takes the keyboard exclusively, and its first field has the focus. The dialog
holds, from the top:

- a key symbol and the title;
- the `description` and the `warning`, when set;
- for a password prompt, a Password field, and with `password-new` a Repeat password field;
- with a `choice-label`, the label and its switch;
- the cancel and continue buttons.

All windows share the fields and the switch. Enter in a field or the continue button answers `yes`;
Escape or the cancel button answers `no`. When choosing a password, the answer goes out only once
both fields match.

## Behind the lock screen

While the screen is locked no prompt is shown; prompts keep waiting and appear once it is unlocked.

With `lock.security.unlockKeyring` on, the default, unlocking the [lock screen](lock.md) with a
password also answers keyring prompts: every waiting password prompt from the owner of
`org.freedesktop.secrets` that does not ask to choose a password is answered once with that
password. A prompt the daemon performs again, for a keyring with another password, then shows as
usual. The same setting has the lock screen unlock the login keyring through the Secret Service.

The switch is on the Screen Lock page under Privacy & Security in the settings, described with the
[power settings](settings-power.md). [Building and installing](installing.md) lists the bus names
proscenio takes.
