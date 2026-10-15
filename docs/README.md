<p align="center">
  <img src="static/img/proscenio-glyph.svg" alt="proscenio" width="160">
</p>

The documentation site for proscenio, a desktop shell for Hyprland written in Rust with GTK 4.
Published at **https://the1fest.github.io/proscenio/**.

## Layout

| Path | Holds |
|---|---|
| `docs/` | The written pages, as plain CommonMark |
| `src/` | The landing page and the theme |
| `static/img/` | The logo |
| `sidebars.ts` | The order the pages appear in |

## Running it

```bash
npm install
npm start
```

Node 20 or newer. `npm run build` produces the static site in `build/`, and `npm run serve` shows what
was built. `npm run typecheck` checks the TypeScript.

## Writing a page

Pages are CommonMark (`markdown.format` is `md`, not MDX) with front matter for the title, the
sidebar label and the description. Add a new page to `sidebars.ts`; nothing is picked up on its own.

Links between pages are relative and keep the `.md` suffix, so they work both on the site and when
reading the file on GitHub:

```markdown
See [The sidebar](sidebar.md) and [IPC](ipc.md).
```

The build runs with `onBrokenLinks: 'throw'` and `onBrokenAnchors: 'throw'`, so a link that does not
resolve fails it.

A page describes proscenio as it is: what the user sees, the setting that changes it, and how it
works, checked against the source. Name the config key in `~/.config/proscenio/config.toml` where a
behavior is configurable.

## Deploying

`.github/workflows/docs.yml` builds this folder and publishes it through GitHub Pages on every push
to `main` that touches `docs/`. Set **Settings → Pages → Source** to *GitHub Actions* once.
