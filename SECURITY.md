# Security policy

## Supported versions

Only the latest release on Flathub (stable, and beta while the `0.99.x` development builds run)
receives fixes. The legacy `0.7.x` Python application is not maintained any more.

## Reporting a vulnerability

Report it privately through GitHub's [private vulnerability
reporting](https://github.com/leinardi/gx52/security/advisories/new), not in a public issue or pull
request. Include the GX52 version, how it was installed, what the issue exposes or weakens, and how
to reproduce it.

GX52 is maintained in spare time, so reports are handled on a best-effort basis. You will get an
answer in the advisory, and the fix is credited there unless you prefer otherwise.

## Scope

In scope: anything GX52 itself does wrong, such as a crafted profile file or legacy database that
makes the application crash, write outside its configuration directory or send unintended commands
to the device; or a Flatpak permission wider than the application needs.

Out of scope: the host udev rule the user installs by hand (it only grants the logged-in user access
to the joystick), and vulnerabilities in the GNOME runtime, which Flathub updates.

## Security model

- GX52 runs as a Flatpak. It needs `--device=all` to reach the joystick's USB and input device
  nodes; it has no network access and no host filesystem access beyond its own configuration
  directory.
- Device access on the host comes from a udev rule with `TAG+="uaccess"`, which gives only the user
  of the active seat access to the X52 vendor and product IDs. The application never asks for root
  and never installs the rule itself.
- The application only sends the vendor control requests the X52 protocol defines, and only to
  devices whose vendor and product ID it recognises.
- Profiles are plain TOML files in `$XDG_CONFIG_HOME/gx52/profiles/`; imported or hand-edited files
  are validated before use.
