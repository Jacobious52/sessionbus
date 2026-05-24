# Release

Sessionbus is not packaged as a cloud service. Releases should make the local
daemon and `aictx` CLI easy to install, inspect, and remove.

## Local Install

```bash
PREFIX="$HOME/.local" ./scripts/install.sh
export PATH="$HOME/.local/bin:$PATH"
aictx setup
```

For a non-mutating preview:

```bash
DRY_RUN=1 ./scripts/install.sh
```

## Shell Completions

```bash
aictx completions zsh > ~/.zfunc/_aictx
aictx completions bash > ~/.local/share/bash-completion/completions/aictx
aictx completions fish > ~/.config/fish/completions/aictx.fish
```

## Homebrew Formula Notes

A future tap formula can build from source:

```ruby
class Sessionbus < Formula
  desc "Local-first continuity infrastructure for AI-assisted engineering work"
  homepage "https://github.com/Jacobious52/sessionbus"
  url "https://github.com/Jacobious52/sessionbus/archive/refs/tags/v0.1.0.tar.gz"
  license "Apache-2.0 OR MIT"
  depends_on "rust" => :build

  def install
    system "cargo", "install", "--path", "crates/aictx-cli", "--bin", "aictx", "--root", prefix
  end

  test do
    system "#{bin}/aictx", "--help"
  end
end
```

## v0.1.0 Checklist

- `bun install --frozen-lockfile`
- `bun run selftest`
- `cargo package -p aictx-cli --allow-dirty`
- `aictx completions zsh`
- `DRY_RUN=1 ./scripts/install.sh`
- README screenshot is current.
- GitHub Actions is green on `main`.
- Create tag `v0.1.0`.
- Publish release notes with install, setup, MCP, dashboard, and privacy notes.
