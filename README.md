# Rust Screen Recorder

A lightweight and efficient screen recorder built using Rust, leveraging `ffmpeg` for high-quality video capture.
## To Do list
- fix seg fault for selecting out video out put
-
-
-
-

## Features
- 🖥️ **Screen Recording**: Capture your desktop with high performance.
- 🎥 **FFmpeg Integration**: Supports multiple video formats.
- 🔊 **Audio Recording**: Record system audio using PipeWire. Coming soon =)
- ⚡ **Optimized Performance**: Low CPU and memory usage.

## Installation
### Prerequisites
Ensure you have the following dependencies installed:
- Rust (stable) → Install via [rustup](https://rustup.rs/)
#### Ubuntu/Debian
```
sudo apt install libgtk-4-dev libglib2.0-dev libgio-dev
```
#### Arch Linux
```
sudo pacman -S gtk4 glib2
```
#### fedora
    sudo dnf install gtk4-devel glib2-devel gio-devel

## Building & Running

    cargo build

# Run the Project

    cargo run

# Build for Release

    cargo build --release

# Running the Binary from Anywhere

    sudo mv target/release/rustcord /usr/local/bin/

Now you can run it from anywhere:

    rustcord

#2. (Optional) Use Cargo's Bin Directory

Alternatively, move it to Cargo's bin directory:

    mv target/release/rustcord ~/.cargo/bin/

Ensure ~/.cargo/bin is in your PATH:

    echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.bashrc

    source ~/.bashrc

Now, you can run:

    rustcord

![Description](https://raw.githubusercontent.com/s-b-repo/rustcord/main/asdasd.png)

## Contributing
1. Fork the repo 🍴
2. Create a new branch `git checkout -b feature-name`
3. Commit changes `git commit -m "Add new feature"`
4. Push `git push origin feature-name`
5. Open a Pull Request 🚀

## License
MIT License. See `LICENSE` for details.

---
Made with ❤️ using Rust!

