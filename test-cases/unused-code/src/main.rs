fn unused() {
    this_is_required_to_error_haha();
}
fn main() {
    other::unused();
}

mod other;
