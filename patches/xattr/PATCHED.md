xattr 0.2.3 from crates.io (MIT/Apache-2.0), with one change in `src/sys/mod.rs`: on Linux/Android
`ENOATTR` is defined as `libc::ENODATA`, because current `libc` no longer exports `ENOATTR` there.
