
let up = container::up("web", true);
if up.trim() != "" {
    fs::write_file("up.txt", up);
}
let shell = container::shell("web", "printf helper-shell");
fs::write_file("shell.txt", shell);
let down = container::down("web");
fs::write_file("down.txt", down);
