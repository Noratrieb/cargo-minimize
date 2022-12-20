# cargo-minimize

Install with `cargo install --git https://github.com/Nilstrieb/cargo-minimize` and use with `cargo minimize`.

## Idea

When encountering problems like internal compiler errors, it's often desirable to have a minimal reproduction that can be used by the people fixing the issue. Usually, these problems are found in big codebases. Getting from a big codebase to a small (<50 LOC) reproduction is non-trivial and requires a lot of manual work. `cargo-minimize` assists you with doing some minimization steps that can be easily automated for you.

## How to use

For minimizing an internal compiler error on a normal cargo project, `cargo minimize` works out of the box. There are many configuration options available though.

```
Usage: cargo minimize [OPTIONS] [PATH]

Arguments:
  [PATH]  The directory/file of the code to be minimited [default: src]

Options:
      --cargo-args <CARGO_ARGS>    Additional arguments to pass to cargo, seperated by whitespace
      --no-color                   To disable colored output
      --rustc                      This option bypasses cargo and uses rustc directly. Only works when a single file is passed as an argument
      --no-verify                  Skips testing whether the regression reproduces and just does the most aggressive minimization. Mostly useful for testing an demonstration purposes
      --verify-fn <VERIFY_FN>      A Rust closure returning a bool that checks whether a regression reproduces. Example: `--verify_fn='|output| output.contains("internal compiler error")'`
      --env <ENV>                  Additional environment variables to pass to cargo/rustc. Example: `--env NAME=VALUE --env ANOTHER_NAME=VALUE`
      --project-dir <PROJECT_DIR>  The working directory where cargo/rustc are invoked in. By default, this is the current working directory
      --script-path <SCRIPT_PATH>  NOTE: This is currently broken. A path to a script that is run to check whether code reproduces. When it exits with code 0, the problem reproduces
  -h, --help                       Print help information
  ```
  