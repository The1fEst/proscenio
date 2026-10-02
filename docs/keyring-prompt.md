# The keyring prompter

qs has no counterpart: gnome-keyring's prompts come from `gcr-prompter`. proscenio takes
that role, so keyring and other gcr prompts (gnome-keyring unlocking a collection, choosing
a new keyring's password, pinentry-gnome3) open in the shell's own dialog.

## 1. The prompter

`src/services/prompter.rs` owns `org.gnome.keyring.SystemPrompter` and
`org.gnome.keyring.PrivatePrompter` on the session bus with `REPLACE`, and exports
`org.gnome.keyring.internal.Prompter` at `/org/gnome/keyring/Prompter`. While proscenio runs,
the bus starts no `gcr-prompter`.

A caller drives one prompt per callback object:

1. `BeginPrompting(o callback)`: the prompter makes a secret exchange and calls the callback's
   `PromptReady("", {}, exchange)` with its public key.
2. `PerformPrompt(o callback, s type, a{sv} properties, s exchange)`: `type` is `password` or
   `confirm`. The properties update the prompt (`title`, `message`, `description`, `warning`,
   `password-new`, `choice-label`, `choice-chosen`, `continue-label`, `cancel-label`), and the
   exchange carries the caller's public key. The prompt waits for an answer.
3. The answer is `PromptReady(reply, changed, exchange)`: `reply` is `yes` or `no`; `changed`
   holds `choice-chosen` when the prompt shows a choice, and `password-strength` (0 for an empty
   password, otherwise 1) when it asks for a new one; for a password, the exchange carries it
   encrypted.
4. The caller may perform again (gnome-keyring does after a wrong password, with `warning`
   set) or call `StopPrompting(o callback)`, answered with `PromptDone()`.

A caller that leaves the bus loses its prompts. A second `BeginPrompting` on the same callback
fails with `org.gnome.keyring.Prompter.Failed`, a `PerformPrompt` while one waits with
`org.gnome.keyring.Prompter.InProgress`.

## 2. The secret exchange

`src/platform/secretexchange.rs` speaks gcr's `sx-aes-1`. Each side sends a key file:

```
[sx-aes-1]
public=<base64>
secret=<base64>
iv=<base64>
```

`public` is a Diffie-Hellman public key in the 1536-bit MODP group of RFC 3526 with
generator 2, big-endian without leading zeros. The shared secret, padded to 192 bytes, goes
through HKDF-SHA256 without salt or info to a 16-byte key. `secret` is the password encrypted
with AES-128-CBC and PKCS#7 padding under that key, `iv` its 16 random bytes; both are left
out when no secret travels.

## 3. The window

`src/panels/keyringprompt.rs` shows the first waiting prompt while the screen is unlocked:
one layer-shell window per monitor, overlay layer, anchored to every edge, namespace
`proscenio:keyring`. The window on Hyprland's focused monitor takes the keyboard
exclusively. Each holds a `WindowDialog` 450 px wide:

- the `key` symbol, 26 px, `colSecondary`, centered;
- the title: `message`, else `title`, else "Authentication", centered;
- `description` in `colOnSurfaceVariant` and `warning` in `colError`, when set;
- for a password prompt, a password field, and a "Repeat password" field when
  `password-new` is set;
- with a `choice-label`, the label and a switch for `choice-chosen`;
- the buttons `cancel-label` (or "Cancel") and `continue-label` (or "OK"), mnemonic
  underscores removed.

All screens share the fields and the switch. Enter in a field and the continue button
answer `yes`; Escape and the cancel button answer `no`. A new password is only sent when
both fields match.

## 4. Prompts behind the lock screen

While the screen is locked no prompt is shown. When the lock screen is unlocked with a
password and `lock.security.unlockKeyring` is on, every waiting password prompt from the
owner of `org.freedesktop.secrets` that does not ask for a new password is answered once
with that password; a prompt the daemon performs again, for a keyring with another
password, then shows as usual. The rest show once the screen is unlocked.
