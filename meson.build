project(
    'gx52',
    'rust',
    # The release workflow sets the version, together with Cargo.toml's (docs/release.md).
    version: '0.0.0',
    meson_version: '>= 1.1',
    license: 'GPL-3.0-or-later',
)

i18n = import('i18n')
gnome = import('gnome')
fs = import('fs')

base_id = 'com.leinardi.gx52'

# Presence/version checks only: cargo links the libraries itself. Keep these in sync with the gtk
# and adw features in Cargo.toml and the runtime-version of the Flatpak manifests (build-aux/).
dependency('glib-2.0', version: '>= 2.88')
dependency('gtk4', version: '>= 4.22')
dependency('libadwaita-1', version: '>= 1.9')

cargo = find_program('cargo')
glib_compile_resources = find_program('glib-compile-resources')
glib_compile_schemas = find_program('glib-compile-schemas')
blueprint_compiler = find_program('blueprint-compiler', version: '>= 0.16')
# Required, not optional: an optional validator that is missing would silently drop its test from
# `meson test`. Both ship in the GNOME SDK; on the host they come with desktop-file-utils and
# appstream (AGENTS.md has the install command).
desktop_file_validate = find_program('desktop-file-validate', required: false)
if not desktop_file_validate.found()
    error(
        'desktop-file-validate not found: meson test validates the desktop file with it. Install desktop-file-utils.',
    )
endif
appstreamcli = find_program('appstreamcli', required: false)
if not appstreamcli.found()
    error(
        'appstreamcli not found: meson test validates the metainfo with it. Install appstream.',
    )
endif

version = meson.project_version()
prefix = get_option('prefix')
bindir = prefix / get_option('bindir')
localedir = prefix / get_option('localedir')
datadir = prefix / get_option('datadir')
pkgdatadir = datadir / meson.project_name()
iconsdir = datadir / 'icons'
podir = meson.project_source_root() / 'po'
gettext_package = meson.project_name()

if get_option('profile') == 'development'
    profile = 'Devel'
    application_id = base_id + '.Devel'
    vcs_tag = ''
    if fs.is_dir('.git')
        vcs_tag = run_command(
            'git',
            'rev-parse',
            '--short',
            'HEAD',
            check: false,
        ).stdout().strip()
    endif
    version_suffix = vcs_tag == '' ? '-devel' : '-' + vcs_tag
else
    profile = ''
    application_id = base_id
    version_suffix = ''
endif

subdir('data')
subdir('po')
subdir('src')

gnome.post_install(
    glib_compile_schemas: true,
    gtk_update_icon_cache: true,
    update_desktop_database: true,
)
