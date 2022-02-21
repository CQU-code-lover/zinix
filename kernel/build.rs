use std::process::Command;

fn main() {
    Command::new("cp").args(&["platform/qemu/linker.ld", "."]).status().unwrap();
}