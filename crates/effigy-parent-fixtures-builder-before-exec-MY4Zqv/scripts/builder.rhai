
let up = container::up("builder", true);
if up.trim() != "" {
    fs::write_file("up.txt", up);
}
fs::write_file("state/arm-replacement", "armed");
let shell = container::shell("builder", "printf builder-shell");
fs::write_file("shell.txt", shell);
