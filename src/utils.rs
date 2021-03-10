#[macro_export]
macro_rules! assert_almost_eq {
    ($left:expr, $right:expr) => {{
        use float_cmp::ApproxEq;
        match ($left, $right) {
            (left_val, right_val) => {
                if !left_val.approx_eq(right_val, (0.0, 2)) {
                    panic!(
                        "assertion failed: `(left == right)`\n      left: `{:?}`,\n     right: `{:?}`",
                        left_val, right_val
                    )
                }
            }
        }
    }};
}
