
let up = container::up("builder", true);
if up.trim() != "" {
    fs::write_file("up.txt", up);
}
let shell = container::shell("builder", "printf builder-shell");
fs::write_file("shell.txt", shell);
