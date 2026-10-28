cargo_target_dir = meson.project_build_root() / 'cargo-target'
cargo_vars = {
  # Keep cargo's caches inside the build dir: reproducible and Flatpak friendly.
  'CARGO_HOME': meson.project_build_root() / 'cargo-home',
  'CARGO_TARGET_DIR': cargo_target_dir,
  # Build-time constants read by src/config.rs through option_env!().
  'APP_ID': application_id,
  'PROFILE': profile,
  'VERSION': version + version_suffix,
  'GETTEXT_PACKAGE': gettext_package,
  'LOCALEDIR': localedir,
  'PKGDATADIR': pkgdatadir,
}
cargo_env = environment(cargo_vars)

cargo_options = ['--manifest-path', meson.project_source_root() / 'Cargo.toml']

if get_option('profile') == 'default'
  cargo_options += ['--release']
  rust_target = 'release'
  message('Building in release mode')
else
  rust_target = 'debug'
  message('Building in debug mode')
endif

custom_target(
  'cargo-build',
  build_by_default: true,
  # Meson cannot see Rust dependencies: always run cargo and let it decide.
  build_always_stale: true,
  output: meson.project_name(),
  console: true,
  install: true,
  install_dir: bindir,
  depends: resources,
  env: cargo_env,
  command: [
    cargo, 'build', cargo_options,
    # cargo --artifact-dir is unstable, so copy the binary out ourselves.
    '&&', 'cp', cargo_target_dir / rust_target / meson.project_name(), '@OUTPUT@',
  ],
)

run_target(
  'cargo-clippy',
  env: cargo_env,
  command: [cargo, 'clippy', cargo_options, '--all-targets', '--', '-D', 'warnings'],
)

test(
  'cargo-test',
  cargo,
  args: ['test', cargo_options],
  # Tests use the build-dir schema and never touch the user's real settings.
  env: cargo_vars + {
    'GSETTINGS_SCHEMA_DIR': meson.project_build_root() / 'data',
    'GSETTINGS_BACKEND': 'memory',
  },
  timeout: 600,
  is_parallel: false,
)

# `meson devenv -C _build` exports the same environment, so plain cargo runs share the caches.
meson.add_devenv(cargo_env)
