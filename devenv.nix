{ pkgs, lib, config, ... }:

let
  # Playwright browsers come from nixpkgs; the npm package must match this version
  # (checked by `check-playwright-version`). Only the `e2e` profile pulls the
  # browsers in: they weigh ~1 GB and E2E runs in CI, not on the dev box.
  playwright = pkgs.playwright-driver;
in
{
  name = "infra-plot";

  packages = [
    pkgs.git
    pkgs.jq
    pkgs.cargo-deny
    pkgs.cargo-watch
    pkgs.taplo
    pkgs.biome
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    # rust-analyzer / rust-src live in the `ide` profile.
    components = [ "rustc" "cargo" "clippy" "rustfmt" ];
    lsp.enable = lib.mkDefault false;
  };

  languages.javascript = {
    enable = true;
    lsp.enable = lib.mkDefault false;
    package = pkgs.nodejs_24;
    pnpm = {
      enable = true;
      install.enable = true;
    };
  };

  env = {
    # Evaluating the version doesn't build or fetch the browsers.
    PLAYWRIGHT_NIX_VERSION = playwright.version;
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
    # The npm biome binary is dynamically linked and won't run on NixOS; the npm
    # package (pinned to the same version) only provides the JS API / schema.
    BIOME_BINARY = "${pkgs.biome}/bin/biome";
    INFRAPLOT_DATA_DIR = "${config.devenv.state}/data";
    INFRAPLOT_STATIC_DIR = "${config.devenv.root}/web/dist";
    RUST_LOG = "infraplot=debug,tower_http=info";
    # Keep caches next to the repo (SSD) instead of $HOME (SD card).
    CARGO_HOME = "${config.devenv.state}/cargo";
    npm_config_store_dir = "${config.devenv.state}/pnpm-store";
    # Dev ports (> 30000). Vite proxies /api to the Rust server.
    INFRAPLOT_WEB_PORT = "31173";
    INFRAPLOT_API_PORT = "31080";
    INFRAPLOT_BIND = "127.0.0.1:31080";
  };

  scripts = {
    gen-schema.exec = ''
      set -euo pipefail
      cd "$DEVENV_ROOT"
      cargo run --quiet -p infraplot-model --bin export-schema > schema/diagram.schema.json
      pnpm --filter web run gen:types
    '';
    check-playwright-version.exec = ''
      set -euo pipefail
      npm_version=$(jq -r '.devDependencies["@playwright/test"]' "$DEVENV_ROOT/e2e/package.json")
      if [ "$npm_version" != "$PLAYWRIGHT_NIX_VERSION" ]; then
        echo "@playwright/test ($npm_version) != nixpkgs playwright-driver ($PLAYWRIGHT_NIX_VERSION)" >&2
        exit 1
      fi
    '';
    lint.exec = ''
      set -euo pipefail
      cd "$DEVENV_ROOT"
      cargo fmt --all -- --check
      cargo clippy --workspace --all-targets --locked -- -D warnings
      pnpm -r run typecheck
      pnpm exec biome ci .
      taplo fmt --check
      check-playwright-version
    '';
    build.exec = ''
      set -euo pipefail
      cd "$DEVENV_ROOT"
      pnpm --filter web run build
      cargo build --release --locked -p infraplot-server
    '';
    dev.exec = ''
      set -euo pipefail
      cd "$DEVENV_ROOT"
      [ -d node_modules ] || pnpm install --frozen-lockfile
      [ -f schema/diagram.schema.json ] || gen-schema
      cargo watch -q -w crates -x 'run -p infraplot-server' &
      server=$!
      trap 'kill $server 2>/dev/null' EXIT INT TERM
      pnpm --filter web exec vite --host 0.0.0.0 --port "$INFRAPLOT_WEB_PORT" --strictPort
    '';
    e2e.exec = ''
      set -euo pipefail
      if [ -z "''${PLAYWRIGHT_BROWSERS_PATH:-}" ]; then
        echo "e2e needs the browsers: devenv --profile e2e shell -- e2e (normally CI runs it)" >&2
        exit 1
      fi
      cd "$DEVENV_ROOT"
      build
      pnpm --filter e2e exec playwright test "$@"
    '';
  };

  processes = {
    server.exec = "cargo watch -w crates -x 'run -p infraplot-server'";
    web.exec = "pnpm --filter web exec vite --host 0.0.0.0";
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    taplo.enable = true;
    biome = {
      enable = true;
      entry = lib.mkForce "${pkgs.biome}/bin/biome check --write --no-errors-on-unmatched";
    };
  };

  profiles = {
    # Headless Chromium for Playwright. Used by CI; locally: devenv --profile e2e shell
    e2e.module = {
      env = {
        PLAYWRIGHT_BROWSERS_PATH = "${playwright.browsers}";
        PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
        # Without it headless Chromium aborts as soon as a page uses web fonts.
        FONTCONFIG_FILE = "${pkgs.makeFontsConf { fontDirectories = [ pkgs.dejavu_fonts ]; }}";
      };
    };
    # Editor tooling: devenv --profile ide shell
    ide.module = {
      languages.rust.components = [ "rust-analyzer" "rust-src" ];
      languages.rust.lsp.enable = true;
      languages.javascript.lsp.enable = true;
    };
  };

  enterShell = ''
    echo "infra-plot dev shell · rust $(rustc --version | cut -d' ' -f2) · node $(node --version)"
    echo "  dev (o devenv up) → vite :$INFRAPLOT_WEB_PORT (abre esto) + api :$INFRAPLOT_API_PORT"
    echo "  lint | build | gen-schema · e2e corre en CI (o: devenv --profile e2e shell -- e2e)"
  '';

  enterTest = ''
    lint
  '';
}
