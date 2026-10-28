project(
  'gx52',
  'rust',
  version: '0.1.0',
  meson_version: '>= 1.1',
  license: 'GPL-3.0-or-later',
)

i18n = import('i18n')
gnome = import('gnome')
fs = import('fs')

base_id = 'com.leinardi.gx52'

# Presence/version checks only: cargo links the libraries itself.
# Keep these in sync with the gtk/adw features in Cargo.toml.
dependency('glib-2.0', version: '>= 2.88')
dependency('gtk4', version: '>= 4.24')
dependency('libadwaita-1', version: '>= 1.9')

cargo = find_program('cargo')
glib_compile_resources = find_program('glib-compile-resources')
glib_compile_schemas = find_program('glib-compile-schemas')
blueprint_compiler = find_program('blueprint-compiler', version: '>= 0.16')
desktop_file_validate = find_program('desktop-file-validate', required: false)
appstreamcli = find_program('appstreamcli', required: false)

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
    vcs_tag = run_command('git', 'rev-parse', '--short', 'HEAD', check: false).stdout().strip()
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

meson.add_dist_script(
  'build-aux/dist-vendor.sh',
  meson.project_build_root() / 'meson-dist' / meson.project_name() + '-' + version,
  meson.project_source_root(),
)

gnome.post_install(
  glib_compile_schemas: true,
  gtk_update_icon_cache: true,
  update_desktop_database: true,
)
