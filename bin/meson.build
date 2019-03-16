conf = configuration_data()
conf.set('VERSION', meson.project_version())
conf.set('localedir', join_paths(prefix, get_option('localedir')))
conf.set('PKGDATA_DIR', PKGDATA_DIR)
conf.set('PYTHON', python.find_python().path())
conf.set('PYTHON_DIR', join_paths(prefix, PYTHON_DIR))

configure_file(
  input: 'gx52.in',
  output: 'gx52',
  configuration: conf,
  install: true,
  install_dir: get_option('bindir')
)

gx52 = join_paths(meson.build_root(), 'bin', 'gx52')
run_target('run',
  command: [gx52]
)
