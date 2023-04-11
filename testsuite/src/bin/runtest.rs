#[cfg(not(unix))]
compile_error!("FIXME: This does not support windows yet. I am so sorry.");

fn main() -> anyhow::Result<()> {
    testsuite::full_tests()
}
