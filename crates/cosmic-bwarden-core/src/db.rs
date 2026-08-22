pub mod models;
pub mod persistence;

pub use models::*;
pub use persistence::Db;

#[cfg(test)]
mod secret_tests {
    use super::Secret;
    use zeroize::Zeroize as _;

    #[test]
    fn secret_implements_zeroize_on_drop() {
        fn assert_zod<T: zeroize::ZeroizeOnDrop>() {}
        assert_zod::<Secret>();
    }

    #[test]
    fn secret_zeroize_wipes_plaintext() {
        let mut s = Secret::from("hunter2");
        s.zeroize();
        assert_eq!(s.expose(), "", "zeroize must clear the inner String");
    }
}
