---
title: Installation
description: Install the rin command with Homebrew or cargo, and check Docker and Compose.
---

## Requirements

- **Docker** with **Compose 5.6 or later**: Docker Desktop, or Docker Engine with the
  Compose plugin. Check with `docker compose version`.
- **macOS on Apple silicon, or Linux** (x86-64 or ARM64) for the prebuilt binaries.
  With `cargo` you can build Rin anywhere Rust and Docker run.
- About **2 GB of memory per agent** with the starter team's limits, which you can change
  in [`resources`](/reference/config/#resources).

## Install

### Homebrew

```sh
brew install w3lt/tap/rin
```

This works on macOS (Apple silicon) and Linux, and also installs shell completions.

### cargo

```sh
cargo install --locked tohsaka
```

On crates.io the package is called **`tohsaka`**; the command it installs is **`rin`**.

### From a release

Every [GitHub release](https://github.com/w3lt/rin/releases) has a `rin-<target>.tar.gz`
for `aarch64-apple-darwin`, `x86_64-unknown-linux-musl` and `aarch64-unknown-linux-musl`.
The Linux builds are static. Unpack `rin` somewhere on your `PATH`.

## Check

```sh
rin --version
```

The version matters: a team's `rin.yml` names the exact Rin version it was written for,
and [`rin generate`](/reference/cli/#rin-generate) refuses a file written for another
one. See [`version`](/reference/config/#version).

## Shell completions

Homebrew sets them up for you. Otherwise, print them for your shell and load them the way
your shell expects:

```sh
rin completions zsh  > ~/.zfunc/_rin          # then: fpath+=(~/.zfunc) in .zshrc
rin completions bash > ~/.local/share/bash-completion/completions/rin
rin completions fish > ~/.config/fish/completions/rin.fish
```

`elvish` and `powershell` are supported too.

## Upgrading

Upgrade the binary, then change `version:` in each team's `rin.yml` to the new version and
run `rin generate` and `rin up` again. The images are rebuilt for the new version; agents'
homes and the message log are kept. See [Changing a team](/guides/lifecycle/).
