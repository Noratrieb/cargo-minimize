# cargo-minimize

Install with `cargo install --git https://github.com/Nilstrieb/cargo-minimize --  --package cargo-minimiz` and use with `cargo minimize`.

## Idea

When encountering problems like internal compiler errors, it's often desirable to have a minimal reproduction that can be used by the people fixing the issue. Usually, these problems are found in big codebases. Getting from a big codebase to a small (<50 LOC) reproduction is non-trivial and requires a lot of manual work. `cargo-minimize` assists you with doing some minimization steps that can be easily automated for you.

## How to use

For minimizing an internal compiler error on a normal cargo project, `cargo minimize` works out of the box. There are many configuration options available though.

```
Usage: cargo minimize [OPTIONS] [PATH]

Arguments:
  [PATH]  The directory/file of the code to be minimized [default: src]

Options:
      --cargo-args <CARGO_ARGS>    Additional arguments to pass to cargo, separated by whitespace
      --no-color                   To disable colored output
      --rustc                      This option bypasses cargo and uses rustc directly. Only works when a single file is passed as an argument
      --no-verify                  Skips testing whether the regression reproduces and just does the most aggressive minimization. Mostly useful for testing and demonstration purposes
      --verify-fn <VERIFY_FN>      A Rust closure returning a bool that checks whether a regression reproduces. Example: `--verify-fn='|output| output.contains("internal compiler error")'`
      --env <ENV>                  Additional environment variables to pass to cargo/rustc. Example: `--env NAME=VALUE --env ANOTHER_NAME=VALUE`
      --project-dir <PROJECT_DIR>  The working directory where cargo/rustc are invoked in. By default, this is the current working directory
      --script-path <SCRIPT_PATH>  NOTE: This is currently broken. A path to a script that is run to check whether code reproduces. When it exits with code 0, the problem reproduces
  -h, --help                       Print help information
```

## What it does

`cargo-minimize` is currently fairly simple. It does several passes over the source code. It treats each file in isolation.
First, it applies the pass to everything in the file. If that stops the reproduction, it goes down the tree, eventually trying each candidate
in isolation. It then repeats the pass until no more changes are made by it.

The currently implemented passes are the following:
- `pub` is replaced by `pub(crate)`. This does not have a real minimization effect on its own.
- Bodies are replaced by `loop {}`. This greatly cuts down on the amount of things and makes many functions unused
- Unused imports are removed
- Unused functions are removed (this relies on the first step, as `pub` items are not marked as `dead_code` by rustc)

Possible improvements:
- Delete more kinds of unused items
- Inline small modules
- Deal with dependencies (there is experimental code in the repo that inlines them)
- Somehow deal with traits
- Integrate more fine-grained minimization tools such as `DustMite` or [`perses`](https://github.com/uw-pluverse/perses)
