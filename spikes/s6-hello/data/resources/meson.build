# Blueprint: list outputs explicitly so ninja knows when every .ui exists
# (output: '.' can race the gresource step).
blueprint_files = [
  'ui/shortcuts-dialog.blp',
  'ui/window.blp',
]

ui_files = []
foreach blp : blueprint_files
  ui_files += fs.replace_suffix(fs.name(blp), '.ui')
endforeach

blueprints = custom_target(
  'blueprints',
  input: files(blueprint_files),
  output: ui_files,
  command: [blueprint_compiler, 'batch-compile', '@OUTDIR@', '@CURRENT_SOURCE_DIR@/ui', '@INPUT@'],
)

# The merged metainfo is bundled so AdwAboutDialog::from_appdata() can read it.
metainfo_conf = configuration_data()
metainfo_conf.set('app-id', application_id)
metainfo_conf.set('gettext-package', gettext_package)
metainfo_file = i18n.merge_file(
  input: configure_file(
    input: '../@0@.metainfo.xml.in.in'.format(base_id),
    output: '@BASENAME@',
    configuration: metainfo_conf,
  ),
  output: '@0@.metainfo.xml'.format(application_id),
  po_dir: podir,
  install: true,
  install_dir: datadir / 'metainfo',
)
if appstreamcli.found()
  test(
    'validate-metainfo',
    appstreamcli,
    args: ['validate', '--no-net', '--explain', metainfo_file],
    suite: ['lint'],
  )
endif

gresource_conf = configuration_data()
gresource_conf.set('metainfo-file', fs.name(metainfo_file.full_path()))
resources = gnome.compile_resources(
  'resources',
  configure_file(
    input: 'resources.gresource.xml.in',
    output: '@BASENAME@',
    configuration: gresource_conf,
  ),
  gresource_bundle: true,
  source_dir: [meson.current_build_dir(), meson.current_source_dir()],
  dependencies: [blueprints, metainfo_file],
  install: true,
  install_dir: pkgdatadir,
)
