// A simple Rust program that prints "Hello, World!" to the console.
// Author: Andy Kukuc

extern crate ipadfs;

use ipadfs::IPadFS;
fn main() {
    let fs = IPadFS::new();
    if fs.is_connected() {
        println!("Connected to iPad");
        for file in fs.list_files("/") {
            println!("File: {}", file);
        }
    } else {
        println!("Not connected to iPad");
    }
}
