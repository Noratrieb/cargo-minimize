//! Handles the --verify-fn flag.
//! It takes in a Rust closure like `|str| true` that takes in a `&str` and returns a bool.

use std::{fmt::Debug, str::FromStr};

use anyhow::{Context, Result};
use quote::quote;

type Entrypoint = unsafe extern "C" fn(*const u8, usize) -> bool;

#[derive(Clone, Copy)]
pub struct RustFunction {
    func: Entrypoint,
}

impl FromStr for RustFunction {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::compile(s)
    }
}

fn wrap_func_body(func: &str) -> Result<String> {
    let closure = syn::parse_str::<syn::ExprClosure>(func).context("invalid rust syntax")?;

    let tokenstream = quote! {
        #[no_mangle]
        pub extern "C" fn cargo_minimize_ffi_function(ptr: *const u8, len: usize) -> bool {
            match ::std::panic::catch_unwind(|| __cargo_minimize_inner(ptr, len)) {
                Ok(bool) => bool,
                Err(_) => ::std::process::abort(),
            }
        }

        fn __cargo_minimize_inner(__ptr: *const u8, __len: usize) -> bool {
            let __slice = unsafe { ::std::slice::from_raw_parts(__ptr, __len) };
            let __str = ::std::str::from_utf8(__slice).unwrap();

            (#closure)(__str)
        }
    };

    Ok(tokenstream.to_string())
}

impl RustFunction {
    #[cfg(not(unix))]
    pub fn compile(body: &str) -> Result<Self> {
        Err(anyhow::anyhow!("--verify-fn only works on unix"));
    }

    #[cfg(unix)]
    pub fn compile(body: &str) -> Result<Self> {
        use anyhow::bail;
        use std::io;
        use std::process::Command;
        use std::{ffi::CString, os::unix::prelude::OsStringExt};

        let file = tempfile::tempdir()?;

        let full_file = wrap_func_body(body)?;

        let source_path = file.path().join("source.rs");

        std::fs::write(&source_path, &full_file).context("writing source")?;

        let mut rustc = Command::new("rustc");
        rustc.arg(source_path);
        rustc.args(["--crate-type=cdylib", "--crate-name=helper", "--emit=link"]);
        rustc.current_dir(file.path());

        let output = rustc.output().context("running rustc")?;
        if !output.status.success() {
            let stderr = String::from_utf8(output.stderr)?;
            bail!("Failed to compile code: {stderr}");
        }

        let dylib_path = file.path().join("libhelper.so");

        let os_str = dylib_path.into_os_string();
        let vec = os_str.into_vec();
        let cstr = CString::new(vec)?;

        let dylib = unsafe { libc::dlopen(cstr.as_ptr(), libc::RTLD_LAZY) };

        if dylib.is_null() {
            bail!("failed to open dylib: {}", io::Error::last_os_error());
        }

        let symbol = b"cargo_minimize_ffi_function\0";

        let func = unsafe { libc::dlsym(dylib, symbol.as_ptr().cast()) };

        if func.is_null() {
            bail!("didn't find entrypoint symbol");
        }

        let func = unsafe { std::mem::transmute::<*mut _, Entrypoint>(func) };

        Ok(Self { func })
    }

    pub fn call(&self, output: &str) -> bool {
        let ptr = output.as_ptr();
        let len = output.len();

        unsafe { (self.func)(ptr, len) }
    }
}

impl Debug for RustFunction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustFunction").finish_non_exhaustive()
    }
}
