// macOS 虚拟键码（CGEvent 的 kCGKeyboardEventKeycode）对应的按键名。

#[cfg(target_os = "macos")]

pub(crate) fn key_name(vk: u32, _scan_code: u32, _extended: bool) -> String {
    // 该平台的扫描码字段为空，键码放在 vk 里。
    MAC_KEY_NAMES
        .iter()
        .find(|(code, _)| *code == vk)
        .map(|(_, name)| name.to_string())
        .unwrap_or_default()
}

static MAC_KEY_NAMES: &[(u32, &str)] = &[
    (0, "A"), (1, "S"), (2, "D"), (3, "F"), (4, "H"), (5, "G"), (6, "Z"), (7, "X"), (8, "C"), (9, "V"),
    (11, "B"), (12, "Q"), (13, "W"), (14, "E"), (15, "R"), (16, "Y"), (17, "T"),
    (18, "1"), (19, "2"), (20, "3"), (21, "4"), (22, "6"), (23, "5"), (24, "="), (25, "9"),
    (26, "7"), (27, "-"), (28, "8"), (29, "0"), (30, "]"), (31, "O"), (32, "U"), (33, "["),
    (34, "I"), (35, "P"), (36, "Return"), (37, "L"), (38, "J"), (39, "'"), (40, "K"),
    (41, ";"), (42, "\\"), (43, ","), (44, "/"), (45, "N"), (46, "M"), (47, "."), (48, "Tab"),
    (49, "Space"), (50, "`"), (51, "Delete"), (53, "Escape"), (54, "RightCommand"),
    (55, "Command"), (56, "Shift"), (57, "CapsLock"), (58, "Option"), (59, "Control"),
    (60, "RightShift"), (61, "RightOption"), (62, "RightControl"), (63, "Fn"),
    (96, "F5"), (97, "F6"), (98, "F7"), (99, "F3"), (100, "F8"), (101, "F9"),
    (103, "F11"), (105, "F13"), (107, "F14"), (109, "F10"), (111, "F12"), (113, "F15"),
    (114, "Help"), (115, "Home"), (116, "PageUp"), (117, "ForwardDelete"), (118, "F4"),
    (119, "End"), (120, "F2"), (121, "PageDown"), (122, "F1"), (123, "Left"),
    (124, "Right"), (125, "Down"), (126, "Up"),
];
