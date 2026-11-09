//! Unit tests for the canonical content id's hash and text form.

use super::{ContentId, ContentIdParseError, PREFIX, sha256};

fn hex(digest: &[u8; 32]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[test]
fn sha256_reproduces_the_fips_180_4_vectors() {
    // FIPS 180-4 / NIST CSRC examples: a dependency bump that changed the output fails here.
    assert_eq!(
        hex(&sha256(b"")),
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
        hex(&sha256(b"abc")),
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    assert_eq!(
        hex(&sha256(
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"
        )),
        "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
    );
}

#[test]
fn the_text_form_is_the_prefix_and_64_lowercase_hex_digits_in_output_order() {
    let mut digest = [0u8; 32];
    for (index, byte) in digest.iter_mut().enumerate() {
        *byte = (index as u8).wrapping_mul(17).wrapping_add(0xa0);
    }
    let id = ContentId::from_digest(digest);
    assert_eq!(id.as_str(), format!("{PREFIX}{}", hex(&digest)));
    assert_eq!(id.to_string(), id.as_str());
    assert_eq!(id.as_str().len(), 11 + 64);
    assert_eq!(id.digest(), &digest);
    assert_eq!(ContentId::parse(id.as_str()), Ok(id.clone()));
    assert_eq!(id.as_str().parse::<ContentId>(), Ok(id));
}

#[test]
fn parse_refuses_every_other_spelling() {
    let good = format!("{PREFIX}{}", "0f".repeat(32));
    assert!(ContentId::parse(&good).is_ok());
    for bad in [
        String::new(),
        PREFIX.to_owned(),
        good.to_uppercase(),
        format!("{PREFIX}{}", "0F".repeat(32)),
        format!("{PREFIX}{}", "0f".repeat(31)),
        format!("{PREFIX}{}0", "0f".repeat(32)),
        format!("{PREFIX}{}", "0g".repeat(32)),
        format!(" {good}"),
        format!("{good}\n"),
        format!("oce:canon2:{}", "0f".repeat(32)),
        format!("cxf:fnv1a128:{}", "0f".repeat(16)),
        format!("oce:canon1:{}", "é".repeat(32)),
    ] {
        assert_eq!(ContentId::parse(&bad), Err(ContentIdParseError), "{bad:?}");
    }
}
