# Bruty

- [Bruty](#bruty)
  - [Running (building from source)](#running-building-from-source)
  - [Usage](#usage)
    - [Notes](#notes)

Brute forces the rest of a YouTube link when you have *part* of it, but not the full link. It successfully obtained `3qw99S3e-ak` a valid YT vid link from `3qw99S3` in **272s** (missing 4 chars).

## Running (building from source)

1. Install [Rust](https://www.rust-lang.org/tools/install).
2. Download and extract the repository from [here](https://github.com/skifli/bruty/archive/refs/heads/master.zip). Alternatively, you can clone the repository with [Git](https://git-scm.com/) by running `git clone https://github.com/skifli/bruty` in a terminal.
3. Navigate into the `/src` directory of your clone of this repository.
4. Run the command `cargo build --release` in the terminal to build the program.
5. The compiled binary will be located at `/target/release/`, named **`bruty.exe`** if you are on Windows, else **`bruty`**.

## Usage

> [!NOTE]
> You have to do them in order, I'm not spending more time to add proper argparsing lol.

| Argument | Description                                                                                            | Default    |
| -------- | ------------------------------------------------------------------------------------------------------ | ---------- |
| 1        | The target YouTube **ID** to start permutations from.                                                  | `3qw99S3`  |
| 2        | The number of **threads** to use.                                                                      | `100`      |
| 3        | Bound for permutations channel before blocking more until a thread is free. Prevents resource hogging. | `10000000` |

### Notes

* A `String` is always 24 bytes (according to [this code](https://dhghomon.github.io/easy_rust/Chapter_14.html#:~:text=A%20String%20is%20always%2024%20bytes), which yes I did run to test). 24 bytes * 10000000 = 240000000 bytes which is 240 MB. This is the maximum amount of memory that can be used to store permutations that are awaiting checking. 2.4GB seemed too much (although it would be fine on most, including my, system).