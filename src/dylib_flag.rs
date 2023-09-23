//! Handles the --verify-fn flag.
//! It takes in a Rust closure like `|str| true` that takes in a `&str` and returns a bool.

use std::{fmt::Debug, mem::ManuallyDrop, str::FromStr};

use anyhow::{Context, Result};
use libloading::Symbol;

#[repr(C)]
pub struct RawOutput {
    out_ptr: *const u8,
    out_len: usize,
    out_has_status: bool,
    out_status: i32,
}

type CheckerCFn = unsafe extern "C" fn(*const RawOutput) -> bool;

#[derive(Clone, Copy)]
pub struct RustFunction {
    func: CheckerCFn,
}

impl FromStr for RustFunction {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::compile(s).context("compiling and loading rust function")
    }
}

fn wrap_func_body(func: &str) -> Result<String> {
    let closure = syn::parse_str::<syn::ExprClosure>(func).context("invalid rust syntax")?;

    let syn_file = syn::parse_quote! {
        #[repr(C)]
        pub struct __RawOutput {
            out_ptr: *const u8,
            out_len: usize,
            out_has_status: bool,
            out_status: i32,
        }

        impl __RawOutput {
            unsafe fn as_output<'a>(&self) -> __Output<'a> {
                let slice = unsafe { std::slice::from_raw_parts(self.out_ptr, self.out_len) };
                let out = std::str::from_utf8(slice).unwrap();
                let status = self.out_has_status.then_some(self.out_status);
                __Output {
                    out,
                    status,
                }
            }
        }

        #[derive(Debug, Clone, Copy)]
        struct __Output<'a> {
            out: &'a str,
            status: Option<i32>,
        }

        #[no_mangle]
        pub unsafe extern "C" fn cargo_minimize_ffi_function(raw: *const __RawOutput) -> bool {
            match std::panic::catch_unwind(|| __cargo_minimize_inner(raw)) {
                Ok(bool) => bool,
                Err(_) => std::process::abort(),
            }
        }

        #[allow(unused_parens)]
        unsafe fn __cargo_minimize_inner(__raw: *const __RawOutput) -> bool {
            let __output = __raw.read();
            let __output = __output.as_output();

            fn ascribe_type<'a, F: FnOnce(__Output<'a>) -> bool>(f: F, output: __Output<'a>) -> bool {
                f(output)
            }

            ascribe_type((#closure), __output)
        }
    };

    crate::formatting::format(syn_file)
}

impl RustFunction {
    pub fn compile(body: &str) -> Result<Self> {
        use anyhow::bail;
        use std::process::Command;

        let file = tempfile::tempdir()?;

        let full_file = wrap_func_body(body)?;

        let source_path = file.path().join("source.rs");

        std::fs::write(&source_path, full_file).context("writing source")?;

        let mut rustc = Command::new("rustc");
        rustc.arg(source_path);
        rustc.args(["--crate-type=cdylib", "--crate-name=helper", "--emit=link"]);
        rustc.current_dir(file.path());

        let output = rustc.output().context("running rustc")?;
        if !output.status.success() {
            let stderr = String::from_utf8(output.stderr)?;
            bail!("Failed to compile code: {stderr}");
        }

        // SAFETY: We are loading a simple rust cdylib, which does not do weird things. But we cannot unload Rust dylibs, so we use MD below.
        let dylib = unsafe {
            libloading::Library::new(file.path().join(libloading::library_filename("helper")))
                .context("loading helper shared library")?
        };
        let dylib = ManuallyDrop::new(dylib);

        let func: Symbol<CheckerCFn> = unsafe {
            dylib
                .get(b"cargo_minimize_ffi_function\0")
                .context("failed to find entrypoint symbol")?
        };

        Ok(Self { func: *func })
    }

    pub fn call(&self, output: &str, code: Option<i32>) -> bool {
        let out_ptr = output.as_ptr();
        let out_len = output.len();
        let (out_has_status, out_status) = match code {
            Some(status) => (true, status),
            None => (false, 0),
        };

        let output = RawOutput {
            out_ptr,
            out_len,
            out_has_status,
            out_status,
        };

        unsafe { (self.func)(&output) }
    }
}

impl Debug for RustFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustFunction").finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::RustFunction;

    #[test]
    fn basic_contains_work() {
        let code = r#"|output| output.out.contains("test")"#;

        let function = RustFunction::compile(code).unwrap();

        let output = "this is a test";
        let not_output = "this is not a tst";

        let code = None;

        assert!(function.call(output, code));
        assert!(!function.call(not_output, code));
    }
}
