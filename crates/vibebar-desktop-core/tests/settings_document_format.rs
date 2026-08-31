//! The two clients write `settings.json` in turn. If they disagree about
//! formatting, every handover rewrites the whole file — a diff nobody made.
//!
//! The fixture is not hand-written: it is what the native app actually wrote,
//! copied out of a demo home, including two keys it does not know (which is
//! why it kept them). Anything else would only test this crate against its own
//! idea of the other implementation.

use vibebar_desktop_core::shared::settings_document;

const NATIVE_WRITTEN: &[u8] =
    include_bytes!("fixtures/settings-native-written.json");

#[test]
fn writes_back_exactly_what_the_native_app_wrote() {
    let object = settings_document::from_slice(NATIVE_WRITTEN)
        .expect("the fixture is a JSON object");
    let bytes = settings_document::to_bytes(&object).expect("serialises");

    if bytes != NATIVE_WRITTEN {
        let ours = String::from_utf8_lossy(&bytes);
        let theirs = String::from_utf8_lossy(NATIVE_WRITTEN);
        let first_difference = ours
            .lines()
            .zip(theirs.lines())
            .enumerate()
            .find(|(_, (a, b))| a != b)
            .map(|(line, (a, b))| format!("line {}:\n  ours:   {a:?}\n  native: {b:?}", line + 1))
            .unwrap_or_else(|| {
                format!("same lines, {} vs {} bytes", bytes.len(), NATIVE_WRITTEN.len())
            });
        panic!("round trip is not byte-identical to the native writer\n{first_difference}");
    }
}

#[test]
fn keeps_a_key_this_build_has_never_heard_of() {
    let object = settings_document::from_slice(NATIVE_WRITTEN).expect("object");
    assert!(
        object.contains_key("settingFromAFutureBuild"),
        "the fixture is meant to carry a key no build knows"
    );
}
