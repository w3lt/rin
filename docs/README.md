# Rin documentation

The user documentation for Rin, built with [Starlight](https://starlight.astro.build) and
published to <https://w3lt.github.io/rin/> from `main`.

```sh
pnpm install
pnpm dev       # http://localhost:4321/rin/
pnpm build     # the site, in dist/
pnpm preview   # serve dist/
```

## Layout

Pages are Markdown in `src/content/docs/`; the URL follows the path. The sidebar is listed by
hand in `astro.config.mjs`, so add a new page there too.

| Directory     | For                                                   |
| :------------ | :---------------------------------------------------- |
| `start/`      | First contact: what Rin is, installing, a first team  |
| `guides/`     | Doing one thing: a task from start to finish          |
| `concepts/`   | How Rin works, for readers who want the model         |
| `reference/`  | Complete and exact: every command, field and file     |

## Conventions

- **Links between pages include the base path**: `/rin/guides/team/`, not `/guides/team/`.
  Starlight adds it to the sidebar only.
- **The code is the source of truth.** Commands and flags come from `src/main.rs` and
  `src/host_cli.rs`, `rin.yml` fields and defaults from `rin-schema.json` and
  `src/shared/config.rs`, error messages from where they are raised. Quote error messages
  exactly, so readers can search for them.
- When a change to Rin changes a command, flag, field, default or message, update these pages
  in the same pull request.
- **Examples pin the current version** (`version: 0.1.2` and the `refs/tags/v0.1.2` schema
  URL), because `rin generate` refuses any other. Update them on every release:
  `grep -rn '0\.1\.2' src/content/docs`.
- Plain words, short sentences, present tense. Address the reader as "you".

## Publishing

`.github/workflows/docs.yml` builds the site on every pull request that touches `docs/`, so a
broken page fails the check before it is merged, and on `main` builds it the same way and
deploys it to GitHub Pages.
