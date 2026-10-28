# The Devel build installs the striped icon under the .Devel application id.
icon_source = profile == 'Devel' ? '@0@.Devel.svg'.format(base_id) : '@0@.svg'.format(base_id)

install_data(
  icon_source,
  install_dir: iconsdir / 'hicolor' / 'scalable' / 'apps',
  rename: '@0@.svg'.format(application_id),
)

install_data(
  '@0@-symbolic.svg'.format(base_id),
  install_dir: iconsdir / 'hicolor' / 'symbolic' / 'apps',
  rename: '@0@-symbolic.svg'.format(application_id),
)
