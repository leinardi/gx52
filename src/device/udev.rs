// Copyright 2026 Roberto Leinardi.
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.

//! The udev rule the guidance pages show (docs/protocol.md, "udev rule").

/// Where the rule is installed; the number must stay below 73 for `uaccess` to apply.
pub(crate) const RULE_PATH: &str = "/etc/udev/rules.d/70-gx52.rules";

/// The rule line that gives the logged-in user access to the joystick's USB node: needed for the
/// LEDs, the MFD and the clocks.
pub(crate) const USB_RULE: &str = r#"SUBSYSTEM=="usb", ENV{DEVTYPE}=="usb_device", ATTR{idVendor}=="06a3", ATTR{idProduct}=="0762|0255|075c", TAG+="uaccess""#;

/// The rule line that gives the logged-in user write access to `/dev/uinput`: needed only for
/// the virtual mouse.
pub(crate) const UINPUT_RULE: &str =
    r#"KERNEL=="uinput", SUBSYSTEM=="misc", TAG+="uaccess", OPTIONS+="static_node=uinput""#;

/// Loads the rule and applies it to the existing nodes, with no replug.
pub(crate) const LOAD_COMMAND: &str = "sudo udevadm control --reload && sudo udevadm trigger";

#[cfg(test)]
mod tests {
    use x52::device::{DeviceType, VENDOR_ID};

    use super::*;

    /// The value of `key=="value"` in `rule`.
    fn matched(rule: &str, key: &str) -> String {
        let start = rule
            .find(&format!("{key}==\""))
            .unwrap_or_else(|| panic!("{key} is not matched in {rule}"))
            + key.len()
            + 3;
        let end = rule[start..].find('"').expect("the value is closed") + start;
        rule[start..end].to_owned()
    }

    #[test]
    fn the_usb_rule_matches_exactly_the_supported_devices() {
        let vendor = u16::from_str_radix(&matched(USB_RULE, "ATTR{idVendor}"), 16).unwrap();
        assert_eq!(vendor, VENDOR_ID);
        let mut ruled: Vec<u16> = matched(USB_RULE, "ATTR{idProduct}")
            .split('|')
            .map(|product| u16::from_str_radix(product, 16).unwrap())
            .collect();
        ruled.sort_unstable();
        let supported: Vec<u16> = (0..=u16::MAX)
            .filter(|product| DeviceType::from_ids(VENDOR_ID, *product).is_some())
            .collect();
        assert_eq!(ruled, supported);
    }

    #[test]
    fn the_protocol_documentation_shows_the_same_rule() {
        let protocol = include_str!("../../docs/protocol.md");
        for line in [USB_RULE, UINPUT_RULE] {
            assert!(
                protocol.lines().any(|doc| doc == line),
                "docs/protocol.md lacks {line}"
            );
        }
        assert!(protocol.contains(RULE_PATH));
        assert!(protocol.contains(LOAD_COMMAND));
    }
}
