//! `settings.json` as it exists on disk, rather than as this build understands
//! it.
//!
//! The file has two writers — this app and the native macOS one — in any
//! combination of versions. A whole-file rewrite from a decoded struct deletes
//! every key the writer did not know about, and the loss is invisible until
//! someone notices a setting has quietly reverted. So a write puts back only
//! the keys this process actually changed, onto whatever the file holds at
//! that moment.
//!
//! The rule is `docs/contracts/settings-write-v1.md` in the native repository,
//! which is the reference implementation. This is the other half of it.

use std::collections::BTreeSet;
use std::io;
use std::path::Path;

use serde_json::{Map, Value};

pub type Object = Map<String, Value>;

/// Whether two JSON values mean the same setting.
///
/// Numbers are the reason this is not `==`. The native client parses with
/// `JSONSerialization`, which turns every number into an `NSNumber` and
/// compares them numerically: it reads `1` and `1.0` as the same value, and
/// re-emits both as `1`. Measured, not assumed — and it means no client can
/// keep a number whose exact text matters, because the other one rewrites it
/// on its next save.
///
/// So the comparison matches that rather than comparing tokens. Comparing
/// tokens would have this client see a change every time the native one
/// normalised a number, and report the user's setting as replaced when
/// nothing had happened to it.
pub fn values_equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Number(a), Value::Number(b)) => numbers_equal(a, b),
        (Value::Array(a), Value::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| values_equal(a, b))
        }
        (Value::Object(a), Value::Object(b)) => {
            a.len() == b.len()
                && a.iter()
                    .all(|(key, value)| b.get(key).is_some_and(|other| values_equal(value, other)))
        }
        _ => left == right,
    }
}

fn numbers_equal(left: &serde_json::Number, right: &serde_json::Number) -> bool {
    // Two integers compare exactly, as `NSNumber` does: going through `f64`
    // would call two different values above 2^53 the same.
    if let (Some(a), Some(b)) = (left.as_i64(), right.as_i64()) {
        return a == b;
    }
    if let (Some(a), Some(b)) = (left.as_u64(), right.as_u64()) {
        return a == b;
    }
    match (left.as_f64(), right.as_f64()) {
        (Some(a), Some(b)) => a == b,
        // Neither side is representable: nothing better than the text is left.
        _ => left == right,
    }
}

/// Read the file as an object. `None` when it is absent or is not an object —
/// a caller falls back to its defaults rather than treating a corrupt file as
/// "every setting was cleared".
pub fn read(path: &Path) -> Option<Object> {
    // Size first: a settings file is a few tens of kilobytes, and anything
    // near this is a corrupt or hostile file rather than settings. Checked
    // before reading so a huge one is never held in memory at all.
    let length = std::fs::metadata(path).ok()?.len();
    if length > MAXIMUM_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    from_slice(&bytes)
}

/// Refused rather than truncated. Shared with the native client, which reads
/// the same file.
pub const MAXIMUM_BYTES: u64 = 8 * 1024 * 1024;

pub fn from_slice(bytes: &[u8]) -> Option<Object> {
    if bytes.len() as u64 > MAXIMUM_BYTES {
        return None;
    }
    match serde_json::from_slice::<Value>(bytes) {
        Ok(Value::Object(object)) => Some(object),
        _ => None,
    }
}

/// Which top-level keys differ between two objects, in either direction.
///
/// `owned` is the vocabulary the comparison may speak. Without it every key
/// this build cannot encode looks like a deletion — an encoded settings struct
/// never mentions a key it has never heard of — and the merge would delete
/// exactly what it exists to protect. `None` means both objects speak the same
/// vocabulary, which is true when comparing two states of the file itself.
pub fn changed_keys(
    baseline: &Object,
    current: &Object,
    owned: Option<&BTreeSet<String>>,
) -> BTreeSet<String> {
    let mut changed = BTreeSet::new();
    for (key, value) in current {
        if !baseline.get(key).is_some_and(|existing| values_equal(existing, value)) {
            changed.insert(key.clone());
        }
    }
    // A key that vanished is a removal only if the writer could have written
    // it in the first place.
    for key in baseline.keys() {
        if !current.contains_key(key) && owned.is_none_or(|owned| owned.contains(key)) {
            changed.insert(key.clone());
        }
    }
    changed
}

/// Serialise in the shape the native app writes, byte for byte.
///
/// Not cosmetic. The two clients write the same file in turn, and a different
/// pretty-printer would rewrite every line of it on each handover — a diff
/// nobody made, over a file a user may well be keeping in version control.
/// `JSONSerialization` with `.prettyPrinted` and `.sortedKeys` produces two
/// spaces of indent, `" : "` between a key and its value, and an empty
/// container as a blank line rather than `{}`.
pub fn to_bytes(object: &Object) -> Result<Vec<u8>, serde_json::Error> {
    // serde_json's Map is a BTreeMap unless `preserve_order` is on, so keys
    // are already in the order `.sortedKeys` produces. The workspace does not
    // enable that feature; this fails loudly here if that ever changes.
    debug_assert!(
        object.keys().collect::<Vec<_>>().windows(2).all(|w| w[0] <= w[1]),
        "settings keys are not sorted: serde_json's preserve_order feature would break \
         byte-compatibility with the native writer"
    );
    let mut buffer = Vec::new();
    let mut serializer =
        serde_json::Serializer::with_formatter(&mut buffer, AppleFoundationFormatter::default());
    serde::Serialize::serialize(&Value::Object(object.clone()), &mut serializer)?;
    Ok(buffer)
}

/// Matches `JSONSerialization`'s `.prettyPrinted` output.
#[derive(Default)]
struct AppleFoundationFormatter {
    depth: usize,
    has_value: bool,
}

impl AppleFoundationFormatter {
    fn newline_indent<W: ?Sized + io::Write>(&self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b"\n")?;
        for _ in 0..self.depth {
            writer.write_all(b"  ")?;
        }
        Ok(())
    }

    /// An empty container is `{` newline blank-line `}` at the parent's indent.
    fn close<W: ?Sized + io::Write>(&mut self, writer: &mut W, bracket: u8) -> io::Result<()> {
        self.depth -= 1;
        if self.has_value {
            self.newline_indent(writer)?;
        } else {
            writer.write_all(b"\n")?;
            self.newline_indent(writer)?;
        }
        writer.write_all(&[bracket])
    }
}

impl serde_json::ser::Formatter for AppleFoundationFormatter {
    fn begin_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.depth += 1;
        self.has_value = false;
        writer.write_all(b"[")
    }

    fn end_array<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b']')
    }

    fn begin_array_value<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if !first {
            writer.write_all(b",")?;
        }
        self.newline_indent(writer)
    }

    fn end_array_value<W: ?Sized + io::Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }

    fn begin_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.depth += 1;
        self.has_value = false;
        writer.write_all(b"{")
    }

    fn end_object<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        self.close(writer, b'}')
    }

    fn begin_object_key<W: ?Sized + io::Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if !first {
            writer.write_all(b",")?;
        }
        self.newline_indent(writer)
    }

    fn begin_object_value<W: ?Sized + io::Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b" : ")
    }

    fn end_object_value<W: ?Sized + io::Write>(&mut self, _writer: &mut W) -> io::Result<()> {
        self.has_value = true;
        Ok(())
    }
}
