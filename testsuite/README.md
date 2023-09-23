# testsuite

The test suite works the following way:

We have a bunch of files in `$WORKSPACE/full-tests`, every file is a test. We then run
`cargo-minimize` on that. `~MINIMIZE-ROOT` are required to be present in the minimization,
and we expect `~REQUIRE-DELETED` to be deleted by cargo-minimize.

We use `bin/regression_checked` as our custom script to verify whether it "reproduces", where
for us, "reproduces" means "all roots are present and the code compiles".
