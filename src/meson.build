# Cargo's home and target directory live in the build dir: reproducible and Flatpak friendly. A
# Devel build under flatpak-builder --ccache (the CI flatpak job, make flatpak-build) puts them in
# /run/ccache instead, the directory flatpak-builder keeps between builds, so a rebuild compiles
# the workspace crates and not every dependency. The release profile, Flathub's, never does.
cargo_cache_dir = meson.project_build_root()
if get_option('profile') == 'development' and fs.is_dir('/run/ccache')
    cargo_cache_dir = '/run/ccache/cargo'
endif
cargo_target_dir = cargo_cache_dir / 'cargo-target'
cargo_vars = {
    'CARGO_HOME': cargo_cache_dir / 'cargo-home',
    'CARGO_TARGET_DIR': cargo_target_dir,
    # Build-time constants read by src/config.rs through option_env!(); a plain cargo build falls
    # back to the Devel defaults written there.
    'APP_ID': application_id,
    'PROFILE': profile,
    'VERSION': version + version_suffix,
    'GETTEXT_PACKAGE': gettext_package,
    'LOCALEDIR': localedir,
    'PKGDATADIR': pkgdatadir,
}
cargo_env = environment(cargo_vars)

# --locked everywhere: Cargo.lock is the pin, and a build that rewrote it would ship a dependency
# graph nobody reviewed.
cargo_options = [
    '--manifest-path',
    meson.project_source_root() / 'Cargo.toml',
    '--locked',
]

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
        cargo,
        'build',
        cargo_options,
        '-p',
        'gx52',
        # cargo --artifact-dir is unstable, so copy the binary out ourselves.
        '&&',
        'cp',
        cargo_target_dir / rust_target / meson.project_name(),
        '@OUTPUT@',
    ],
)

run_target(
    'cargo-clippy',
    env: cargo_env,
    command: [
        cargo,
        'clippy',
        cargo_options,
        # The same scope as the cargo-clippy pre-commit hook: every member, every target.
        '--workspace',
        '--all-targets',
        '--all-features',
        '--',
        '-D',
        'warnings',
    ],
)

test(
    'cargo-test',
    cargo,
    # The application and the device library. Not meta: its tests read the git checkout, which a
    # Flatpak build does not have; CI runs them on the bare runner instead.
    args: ['test', cargo_options, '-p', 'gx52', '-p', 'x52'],
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
