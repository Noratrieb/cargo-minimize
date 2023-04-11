fn main() -> anyhow::Result<()> {
    let root_var =
        std::env::var("MINIMIZE_RUNTEST_ROOTS").expect("MINIMIZE_RUNTEST_ROOTS env var not found");
    let roots = root_var.split(",").collect::<Vec<_>>();

    let proj_dir = std::env::current_dir().expect("current dir not found");

    testsuite::ensure_correct_minimization(&proj_dir, roots)
}
