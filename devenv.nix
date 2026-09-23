{ pkgs, lib, config, ... }:

let
  # Playwright browsers come from nixpkgs; the npm package must match this version
  # (checked by `check-playwright-version`).
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
  ];

  languages.rust = {
    enable = true;
    channel = "stable";
    components = [ "rustc" "cargo" "clippy" "rustfmt" "rust-analyzer" "rust-src" ];
  };

  languages.javascript = {
    enable = true;
    package = pkgs.nodejs_24;
    pnpm = {
      enable = true;
      install.enable = true;
    };
  };

  env = {
    PLAYWRIGHT_BROWSERS_PATH = "${playwright.browsers}";
    PLAYWRIGHT_SKIP_BROWSER_DOWNLOAD = "1";
    PLAYWRIGHT_SKIP_VALIDATE_HOST_REQUIREMENTS = "true";
    PLAYWRIGHT_NIX_VERSION = playwright.version;
    INFRAPLOT_DATA_DIR = "${config.devenv.state}/data";
    INFRAPLOT_STATIC_DIR = "${config.devenv.root}/web/dist";
    RUST_LOG = "infraplot=debug,tower_http=info";
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
    e2e.exec = ''
      set -euo pipefail
      cd "$DEVENV_ROOT"
      build
      pnpm --filter e2e exec playwright test "$@"
    '';
  };

  processes = {
    server.exec = "cargo watch -w crates -x 'run -p infraplot-server -- --bind 127.0.0.1:8080'";
    web.exec = "pnpm --filter web run dev";
  };

  git-hooks.hooks = {
    rustfmt.enable = true;
    taplo.enable = true;
    biome = {
      enable = true;
      entry = lib.mkForce "pnpm exec biome check --write --no-errors-on-unmatched";
    };
  };

  enterShell = ''
    echo "infra-plot dev shell · rust $(rustc --version | cut -d' ' -f2) · node $(node --version) · playwright ${playwright.version}"
    echo "  devenv up        → server (:8080) + vite (:5173)"
    echo "  lint | build | e2e | gen-schema"
  '';

  enterTest = ''
    lint
    e2e
  '';
}
