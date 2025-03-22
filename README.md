# Rust Screen Recorder

A lightweight and efficient screen recorder built using Rust, leveraging `pipewire` and `ffmpeg` for high-quality video capture.

## Features
- 🖥️ **Screen Recording**: Capture your desktop with high performance.
- 🎥 **FFmpeg Integration**: Supports multiple video formats.
- 🔊 **Audio Recording**: Record system audio using PipeWire.
- ⚡ **Optimized Performance**: Low CPU and memory usage.

## Installation
### Prerequisites
Ensure you have the following dependencies installed:
- Rust (stable) → Install via [rustup](https://rustup.rs/)
- `ffmpeg`
- `pipewire`
- `libclang`

#### Ubuntu/Debian
```sh
sudo apt update && sudo apt install -y ffmpeg libpipewire-0.3-dev clang
```
#### Arch Linux
```sh
sudo pacman -S ffmpeg pipewire clang
```
#### MacOS
```sh
brew install ffmpeg pipewire llvm
```

## Usage
Run the recorder with default settings:
```sh
target/release/rust-screen-recorder
```
For custom settings:
```sh
target/release/rust-screen-recorder --output video.mp4 --fps 30
```

## Troubleshooting
If you encounter a `clang-sys` conflict, try:
```sh
cargo update -p clang-sys --precise 0.29.0
```

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

