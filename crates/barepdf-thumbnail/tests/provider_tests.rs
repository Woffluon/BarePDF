use barepdf_thumbnail::bitmap::{calculate_thumbnail_dimensions, create_32bit_dib_section};
use barepdf_thumbnail::CLSID_BAREPDF_THUMBNAIL;
use windows::core::GUID;

#[test]
fn test_clsid_is_stable_and_valid() {
    assert_ne!(CLSID_BAREPDF_THUMBNAIL, GUID::zeroed());
    assert_eq!(
        format!("{:?}", CLSID_BAREPDF_THUMBNAIL),
        "4F7B3E21-9C8D-4E15-A2B0-8E9D6F3C1A5B"
    );
}

#[test]
fn test_invalid_bitmap_inputs_return_none() {
    assert!(create_32bit_dib_section(0, 100, &[]).is_none());
    assert!(create_32bit_dib_section(100, 0, &[]).is_none());
    assert!(create_32bit_dib_section(10, 10, &[0u8; 10]).is_none()); // Insufficient pixels (need 400)
}

#[test]
fn test_aspect_ratio_clamping() {
    let (w, h) = calculate_thumbnail_dimensions(1000.0, 10.0, 256);
    assert_eq!(w, 256);
    assert!(h >= 1);
}
