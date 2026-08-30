# Cross-client storage fixtures

The JSON and `.sha256` pairs in this directory are copied byte-for-byte from
`AstroQore/vibe-bar/docs/contracts/`. That native directory is the **only
source of truth**: never edit or regenerate these files from Rust. The settings
fixtures are synthetic and product-disabled, but are still authored on the
native side so Swift and Rust consume the same exact vectors.

`storage_contract` verifies each sidecar's exact SHA-256 before decoding the
fixture, and CI runs those conformance tests. The native test suite performs
the reciprocal byte comparison. A coordinated contract change copies all four
files from native in one change and updates both clients' CI together.

`storage-contract.yml` also builds the non-default Rust synthetic probe and
runs the native Swift interoperability test against the exact native contract
commit named in that workflow. Updating either implementation therefore
requires updating the paired commit and passing the real cross-process lock
matrix, not only matching JSON bytes.

The lease probe exists solely for synthetic temporary-directory interoperability
tests. It does not authorize Desktop to write any legacy shared store and is
absent from the default product build. Create a fresh child of `$TMPDIR`, then
build it explicitly with:

`cargo run -p vibebar-desktop-core --example shared_store_lease_probe --features contract-probe -- --root "$TMPDIR/<fresh-child>" --store quota_cache --mode try`
