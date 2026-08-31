# Cross-client storage contract — design record

**Nothing in this repository implements this contract today, and nothing
validates these files. They are kept as a design record, not as a live spec.**

## What this is

When Desktop eventually writes to the shared Vibe Bar data root, two clients
will be mutating the same files, and the current shared stores are not safe
for that: several respond to a schema mismatch by *deleting* the file, and
every JSON store is a whole-file read-modify-write with no lock. These
documents are a worked design for the coordination protocol that problem
needs — store manifest, single-writer lease record, and a lossless settings
merge with an explicit conflict rule.

The design was derived from the native app's real storage behaviour, so it is
a genuine starting point rather than a sketch. A first Rust implementation and
a matching Swift implementation both existed and passed cross-process tests.

## Why it is not implemented

Desktop is read-only outside its own `client/desktop/` namespace, so it needs
no lease. The native app is the sole writer, so it needs no lease either.
Shipping both halves would have meant carrying a coordination protocol — plus
a new C target on the native side — in two released apps for a capability
neither one exercises. The protocol will also almost certainly change once a
real second writer exists, because that is when the hard cases show up.

So both implementations were removed and this record kept. Recover them from
git when the work is actually scheduled:

- Rust: `git log --diff-filter=D -- 'crates/vibebar-desktop-core/src/storage_contract/*'`
- Swift: the `AstroQore/vibe-bar` branches behind the closed PRs #256, #257, #260

## Reading the fixtures

`shared-store-contract-v1.json` and `shared-store-lease-record-v1.json`
describe the store manifest and the on-disk lease record.
`settings-document-v1.md` plus its three JSON vectors describe the lossless
settings merge, including the case that motivated it: a numeric token such as
`1e-400` must not be normalised to `0`, because two clients would then silently
disagree about whether they had conflicting values.

The `.sha256` sidecars were removed with the implementations — they existed to
prove byte-for-byte sync with a native directory that no longer exists.
