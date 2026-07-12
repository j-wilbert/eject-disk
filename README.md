# eject-disk

A simple command-line utility written in Rust to help you quickly select and eject external disks on macOS.

## Features
- Detects external physical disks and APFS containers.
- Lists mounted volumes on each disk along with their capacities.
- Interactive disk selection using `fzf`.
- Safely ejects the selected disk using macOS's native `diskutil`.

## Prerequisites
- **macOS**
- **[fzf](https://github.com/junegunn/fzf)** (must be installed and available in your `PATH`)

## Usage
Simply run:
```bash
cargo run --release
```

## License
MIT License
