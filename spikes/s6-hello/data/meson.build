subdir('icons')
subdir('resources')

# Desktop file: substitute the app id, then merge translations.
desktop_conf = configuration_data()
desktop_conf.set('app-id', application_id)
desktop_conf.set('bin', meson.project_name())
desktop_file = i18n.merge_file(
  input: configure_file(
    input: '@0@.desktop.in.in'.format(base_id),
    output: '@BASENAME@',
    configuration: desktop_conf,
  ),
  output: '@0@.desktop'.format(application_id),
  type: 'desktop',
  po_dir: podir,
  install: true,
  install_dir: datadir / 'applications',
)
if desktop_file_validate.found()
  test('validate-desktop', desktop_file_validate, args: [desktop_file], suite: ['lint'])
endif

# Settings schema: both the id and the dconf path derive from the application id, so Devel and
# stable builds keep separate settings. dconf stores values by path, not by schema id: two schemas
# with different ids but the same path share every value.
gschema_conf = configuration_data()
gschema_conf.set('app-id', application_id)
gschema_conf.set('app-path', '/' + application_id.replace('.', '/') + '/')
gschema_conf.set('gettext-package', gettext_package)
gschema_file = configure_file(
  input: '@0@.gschema.xml.in'.format(base_id),
  output: '@0@.gschema.xml'.format(application_id),
  configuration: gschema_conf,
  install: true,
  install_dir: datadir / 'glib-2.0' / 'schemas',
)
# Compile the configured schema in the build dir so the app runs uninstalled under `meson devenv`.
# (gnome.compile_schemas() would compile the source dir, which only holds the .in template.)
custom_target(
  'gschemas-compiled',
  input: gschema_file,
  output: 'gschemas.compiled',
  command: [glib_compile_schemas, '--strict', '--targetdir', '@OUTDIR@', '@OUTDIR@'],
  build_by_default: true,
)
test(
  'validate-schema',
  glib_compile_schemas,
  args: ['--strict', '--dry-run', meson.current_build_dir()],
  suite: ['lint'],
)

devenv = environment()
devenv.set('GSETTINGS_SCHEMA_DIR', meson.current_build_dir())
meson.add_devenv(devenv)
