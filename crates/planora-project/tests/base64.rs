//! Base64 für die Netzdaten (RFC 4648, streng; Teilprojekt 9, Task 11).

use planora_project::base64::{Base64Error, decode, decode_prefix, decoded_len, encode};

#[test]
fn rfc_4648_vectors() {
    for (plain, coded) in [
        ("", ""),
        ("f", "Zg=="),
        ("fo", "Zm8="),
        ("foo", "Zm9v"),
        ("foob", "Zm9vYg=="),
        ("fooba", "Zm9vYmE="),
        ("foobar", "Zm9vYmFy"),
    ] {
        assert_eq!(encode(plain.as_bytes()), coded);
        assert_eq!(decode(coded).unwrap(), plain.as_bytes());
        assert_eq!(decoded_len(coded), Ok(plain.len()));
    }
}

#[test]
fn all_byte_values_round_trip() {
    let bytes: Vec<u8> = (0..=255u8).cycle().take(1000).collect();
    assert_eq!(decode(&encode(&bytes)).unwrap(), bytes);
}

#[test]
fn strict_decoding_refuses_everything_else() {
    for bad in [
        "Zg=", "Zg", "Z===", "Zh==", "Zm9=", "Zm 9v", "Zm9v\n", "Zm-v", "Zg==Zg==", "=Zg=", "ä000",
    ] {
        assert_eq!(decode(bad), Err(Base64Error), "{bad:?}");
    }
    assert_eq!(decoded_len("abc"), Err(Base64Error));
}

#[test]
fn a_prefix_is_decoded_without_the_rest() {
    let coded = encode(b"Planora ist ein CAD");
    assert_eq!(decode_prefix(&coded, 8).unwrap(), b"Planora i");
    assert_eq!(decode_prefix("Zg==", 8).unwrap(), b"f");
    assert_eq!(decode_prefix("@@@@AAAA", 2), Err(Base64Error));
    assert_eq!(decode_prefix("ä000Zg==", 1), Err(Base64Error));
}
