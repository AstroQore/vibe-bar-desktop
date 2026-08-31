# Cross-client contracts

Two kinds of file live here, and mixing them up is how a design record gets
mistaken for a rule the code follows.

**Live, implemented and checked by tests in this repository:**

| File | Covers | Checked by |
| --- | --- | --- |
| `settings-write-v1.md` | How either client writes `settings.json` without losing the other's data | `tests/settings_writer.rs`, `tests/shared_file_lock.rs` |
| `settings-value-equality-v1.json` | When two JSON values mean the same setting, and the one place the clients differ | `tests/settings_value_equality_contract.rs` |
| `design-tokens-v1.json` | Provider accents, quota-bar colours, card recipe | the design-token tests |
| `quota-naming-v1.json` | The quota axis: company, SubProvider, group | `naming.rs` |
| `forecast-v1.json` | The quota pace forecast, as evaluated vectors | the forecast tests |

**A design record, implemented by nothing here:** everything below —
`shared-store-contract-v1.json`, `shared-store-lease-record-v1.json`, and
`settings-document-v1.md` with its vectors. `settings-document-v1.md` is
superseded by `settings-write-v1.md`; the reasons two of its decisions were
not carried over are in that file.

## What the design record is

When Desktop writes shared stores *beyond* `settings.json`, two clients will be
mutating files that are not safe for it: several respond to a schema mismatch by *deleting* the file, and
every JSON store is a whole-file read-modify-write with no lock. These
documents are a worked design for the coordination protocol that problem
needs — store manifest, single-writer lease record, and a lossless settings
merge with an explicit conflict rule.

The design was derived from the native app's real storage behaviour, so it is
a genuine starting point rather than a sketch. A first Rust implementation and
a matching Swift implementation both existed and passed cross-process tests.

## Why the lease half is not implemented

Desktop writes exactly one shared store, `settings.json`, and it does so under
an advisory `flock(2)` rather than a lease — see `settings-write-v1.md`. Every
other shared store is still read-only here, and the native app is still their
sole writer, so neither side needs a lease for them.
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
