//! Source order compatible with the identities produced by the retired Bun runtime.
use std::cmp::Ordering;

/// Existing snapshot identities use JavaScript's UTF-16 code-unit order.
pub fn compare_serialized(left: &str, right: &str) -> Ordering {
  left.encode_utf16().cmp(right.encode_utf16())
}

/// Paths are listed in `en-US` ICU collation order, identical on every platform.
pub fn compare_paths(left: &str, right: &str) -> Ordering {
  use icu_collator::{Collator, CollatorBorrowed, options::CollatorOptions};
  use icu_locale_core::locale;
  use std::sync::OnceLock;

  static COLLATOR: OnceLock<CollatorBorrowed<'static>> = OnceLock::new();
  COLLATOR
    .get_or_init(|| {
      Collator::try_new(locale!("en-US").into(), CollatorOptions::default())
        .expect("compiled ICU collation data")
    })
    .compare(left, right)
}
