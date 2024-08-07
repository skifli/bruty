# Bruty

- [Bruty](#bruty)
  - [Running (building from source)](#running-building-from-source)
  - [Usage](#usage)

Brute forces the rest of a YouTube link when you have *part* of it, but not the full link. It successfully obtained [**`TlTxAwYypvsas`**](https://youtube.com/watch?v=TlTxAwYypvsas) a valid YT vid link from **`TlTxAwYy`** in *232s*.

## Running (building from source)

1. Install [Rust](https://www.rust-lang.org/tools/install).
2. Download and extract the repository from [here](https://github.com/skifli/bruty/archive/refs/heads/master.zip). Alternatively, you can clone the repository with [Git](https://git-scm.com/) by running `git clone https://github.com/skifli/bruty` in a terminal.
3. Navigate into the `/src` directory of your clone of this repository.
4. Run the command `cargo build --release` in the terminal to build the program.
5. The compiled binary will be located at `/target/release/`, named **`bruty.exe`** if you are on Windows, else **`bruty`**.

## Usage

| Argument | Description                                           | Default  |
| -------- | ----------------------------------------------------- | -------- |
| 1        | The target YouTube **ID** to start permutations from. | `3qw99S` |
| 2        | The number of **threads** to use.                     | `5`      |