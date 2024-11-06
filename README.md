# Cargo Onefile

Cargo Onefile is a Rust tool that generates a single file containing all the source code of a Rust project, primarily designed for piping source code into Large Language Models (LLMs).

![Rust](https://img.shields.io/badge/language-Rust-orange.svg)
![License](https://img.shields.io/badge/license-MIT-blue.svg)

## Table of Contents

- [Cargo Onefile](#cargo-onefile)
  - [Table of Contents](#table-of-contents)
  - [Installation](#installation)
  - [Usage](#usage)
  - [Features](#features)
  - [Configuration](#configuration)
  - [Contributing](#contributing)
  - [License](#license)
  - [Support](#support)

## Installation

To install Cargo Onefile, you need to have Rust and Cargo installed on your system. If you don't have them installed, follow the instructions on the [official Rust website](https://www.rust-lang.org/tools/install).

Once Rust is installed, you can install Cargo Onefile using the following command:

```sh
cargo install cargo-onefile
```

## Usage

To use Cargo Onefile, navigate to your Rust project directory and run:

```sh
cargo onefile
```

This will generate a single file containing all the source code of your project. By default, the output file will be named `onefile.rs` in the current directory.

For more options, you can use the `--help` flag:

```sh
cargo onefile --help
```

## Features

1. **Single File Generation**: Combines all source files into a single file for easy sharing or analysis.
2. **Flexible Output**: Supports writing to a file or stdout, with customizable output paths.
3. **Dependency Inclusion**: Option to include project dependencies in the output.
4. **Customizable Filtering**: Allows filtering files based on size, modification date, and file extensions.
5. **Performance Metrics**: Includes an info mode to measure performance and provide statistics on the processed files.

## Configuration

Cargo Onefile offers various configuration options:

- `--stdout`: Output to stdout instead of a file.
- `--table-of-contents`: Include a table of contents at the top of the output.
- `-o, --output <PATH>`: Specify the output file path.
- `-p, --manifest-path <PATH>`: Specify the path to the Cargo.toml file.
- `--head <PATH>`: Prepend contents of a header file to the output.
- `--depth <DEPTH>`: Set the maximum depth to search for files.
- `--skip-gitignore <BOOL>`: Choose whether to skip gitignored files.
- `-d, --dependencies`: Include project dependencies in the output.
- `--separator <STRING>`: Set the separator shown between files.
- `--newer-than <DATETIME>`: Exclude files older than the specified datetime.
- `--older-than <DATETIME>`: Exclude files newer than the specified datetime.
- `--smaller-than <SIZE>`: Exclude files larger than the specified size in bytes.
- `--larger-than <SIZE>`: Exclude files smaller than the specified size in bytes.
- `--max-files <NUMBER>`: Set the maximum number of files to include.
- `-E, --extension <EXTENSION>`: Include files with the specified extension(s).
- `-e, --exclude <FILE>`: Exclude specified files from the output.

For a complete list of options, use the `--help` flag.

## Contributing

Contributions to Cargo Onefile are welcome! Please follow these steps to contribute:

1. Fork the repository.
2. Create a new branch for your feature or bug fix.
3. Write your code and tests.
4. Ensure all tests pass by running `cargo test`.
5. Submit a pull request with a clear description of your changes.

Please adhere to the existing code style and include appropriate tests for new features.

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

## Support

For support, questions, or feedback, please [open an issue](https://github.com/exotik850/cargo-onefile/issues) on the GitHub repository.
