//! Source order compatible with the identities produced by the retired Bun runtime.
use std::cmp::Ordering;

/// Existing snapshot identities use JavaScript's UTF-16 code-unit order.
pub fn compare_serialized(left: &str, right: &str) -> Ordering {
  left.encode_utf16().cmp(right.encode_utf16())
}

/// Paths are listed in the `en-US` ICU collation order Bun used.
#[cfg(target_os = "macos")]
pub fn compare_paths(left: &str, right: &str) -> Ordering {
  system::compare(left, right)
}

#[cfg(not(target_os = "macos"))]
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

#[cfg(target_os = "macos")]
mod system {
  // Bun links macOS's ICU directly; use the same ICU4C service here.
  use std::cmp::Ordering;
  use std::ffi::c_char;
  use std::sync::OnceLock;

  #[repr(C)]
  struct UCollator {
    _private: [u8; 0],
  }

  // Safety: these declarations match the stable ICU4C C ABI shipped in the
  // macOS SDK and use an opaque collator pointer only through ICU calls.
  #[link(name = "icucore")]
  unsafe extern "C" {
    fn ucol_open(locale: *const c_char, status: *mut i32) -> *mut UCollator;
    fn ucol_strcollUTF8(
      collator: *const UCollator,
      left: *const c_char,
      left_length: i32,
      right: *const c_char,
      right_length: i32,
      status: *mut i32,
    ) -> i32;
  }

  // The collator is opened once and never freed, so its address is stable.
  static COLLATOR: OnceLock<usize> = OnceLock::new();

  fn collator() -> *const UCollator {
    let address = *COLLATOR.get_or_init(|| {
      let mut status = 0;
      let locale = b"en_US\0";
      // Safety: the locale is a static NUL-terminated C string and the
      // status pointer is valid for the duration of the call.
      let collator = unsafe { ucol_open(locale.as_ptr().cast(), std::ptr::addr_of_mut!(status)) };
      assert!(
        status <= 0 && !collator.is_null(),
        "macOS ICU could not open the en_US collator"
      );
      collator as usize
    });
    address as *const UCollator
  }

  pub(super) fn compare(left: &str, right: &str) -> Ordering {
    let collator = collator();
    let left_length = i32::try_from(left.len()).expect("path fits ICU's byte length");
    let right_length = i32::try_from(right.len()).expect("path fits ICU's byte length");
    let mut status = 0;
    // Safety: Rust strings are valid UTF-8, their explicit byte lengths fit
    // ICU's i32 API, and the collator remains alive in the process singleton.
    let result = unsafe {
      ucol_strcollUTF8(
        collator,
        left.as_ptr().cast(),
        left_length,
        right.as_ptr().cast(),
        right_length,
        std::ptr::addr_of_mut!(status),
      )
    };
    assert!(status <= 0, "macOS ICU failed to compare UTF-8 paths");
    result.cmp(&0)
  }
}
